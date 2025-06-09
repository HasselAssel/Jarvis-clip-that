use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::thread;
use std::thread::JoinHandle;

use ffmpeg_next::codec::{Flags, Parameters};
use ffmpeg_next::encoder::Audio;
use wasapi::{AudioClient, Direction, Handle, WasapiError};

use crate::capturer::capture::recorder::{AudioParams, BaseParams, Recorder};
use crate::capturer::error::IdkCustomErrorIGuess;
use crate::capturer::ring_buffer::PacketRingBuffer;
use crate::com::ComObj;

const AAC_FRAME_SIZE: usize = 1024;
const SAMPLE_RATE: usize = 48_000;

pub struct AudioPerProcess<P: PacketRingBuffer + 'static> {
    audio_encoder: Audio,
    ring_buffer: Arc<Mutex<P>>,

    client: MaybeSafeAudioClient,

    frame: ffmpeg_next::util::frame::audio::Audio,
    empty_frame: ffmpeg_next::util::frame::audio::Audio,

    event: MaybeSafeHANDLE,
}

impl<P: PacketRingBuffer> AudioPerProcess<P> {
    pub fn new(p_id: u32, include_tree: bool, ring_buffer: Arc<Mutex<P>>) -> Result<(Self, Parameters), WasapiError> {
        unsafe {
            windows::Win32::System::Com::CoInitializeEx(None, windows::Win32::System::Com::COINIT_MULTITHREADED).unwrap();
        }

        let mut client = MaybeSafeAudioClient(AudioClient::new_application_loopback_client(p_id, include_tree)?);

        let desired_format = wasapi::WaveFormat::new(32, 32, &wasapi::SampleType::Float, SAMPLE_RATE, 2, None);
        let buffer_duration = 10_000_000; // 1s buffer in 100ns units
        let mode = wasapi::StreamMode::EventsShared { autoconvert: true, buffer_duration_hns: buffer_duration };

        client.initialize_client(&desired_format, &Direction::Capture, &mode)?;

        let event = MaybeSafeHANDLE(client.set_get_eventhandle()?);


        let audio_params = AudioParams{
            base_params: BaseParams {
                codec: ffmpeg_next::codec::encoder::find(ffmpeg_next::codec::Id::AAC).ok_or(ffmpeg_next::Error::EncoderNotFound).unwrap(),
                bit_rate: 128_000,
                max_bit_rate: 150_000,
                flags: Flags::GLOBAL_HEADER,
                rate: SAMPLE_RATE as i32,
            },
            channel_layout: ffmpeg_next::util::channel_layout::ChannelLayout::STEREO,
            format: ffmpeg_next::format::Sample::F32(ffmpeg_next::util::format::sample::Type::Planar),
        };

        let codec = audio_params.base_params.codec;
        let ctx = ffmpeg_next::codec::context::Context::new_with_codec(codec);
        let mut enc = ctx.encoder().audio().unwrap();

        enc.set_rate(audio_params.base_params.rate);
        enc.set_channel_layout(audio_params.channel_layout);
        enc.set_format(audio_params.format);
        enc.set_time_base((1, audio_params.base_params.rate));
        enc.set_flags(audio_params.base_params.flags);

        let audio_encoder = enc.open_as(codec).unwrap();

        let par = Parameters::from(&audio_encoder);

        let frame = ffmpeg_next::util::frame::audio::Audio::new(
            audio_encoder.format(),
            AAC_FRAME_SIZE,
            audio_encoder.channel_layout(),
        );
        let mut empty_frame = ffmpeg_next::util::frame::audio::Audio::new(
            audio_encoder.format(),
            AAC_FRAME_SIZE,
            audio_encoder.channel_layout(),
        );
        let buf = vec![0u8; AAC_FRAME_SIZE * empty_frame.format().bytes() * empty_frame.channel_layout().channels() as usize];
        Self::copy_into_frame(&mut empty_frame, buf);


        Ok((Self {
            audio_encoder,
            ring_buffer,
            client,

            frame,
            empty_frame,

            event
        }, par))
    }
}

impl <P: PacketRingBuffer> Recorder<P> for AudioPerProcess<P> {
    fn start_capturing(mut self) -> JoinHandle<Result<(), IdkCustomErrorIGuess>> {
        thread::spawn(move || -> Result<(), IdkCustomErrorIGuess> {
            let capture_client = self.client.get_audiocaptureclient()?;

            self.client.start_stream()?;

            let mut pts_counter: i64 = 0;
            let mut total_buffer = VecDeque::new();

            let mut freq = 0;
            let mut start_time = 0;
            let mut now_time_buf = 0;
            unsafe {
                windows::Win32::System::Performance::QueryPerformanceFrequency(&mut freq)?;
                windows::Win32::System::Performance::QueryPerformanceCounter(&mut start_time)?;
            }

            loop {
                self.event.wait_for_event(4294967295).unwrap();

                unsafe { windows::Win32::System::Performance::QueryPerformanceCounter(&mut now_time_buf)?; }
                let new_pts = (now_time_buf - start_time) * SAMPLE_RATE as i64 / freq;
                let diff = (new_pts - pts_counter).max(0);
                if diff >= AAC_FRAME_SIZE as _ {
                    //println!("ADDED SILENCE!!!");
                    self.flush_and_silence(&mut total_buffer, diff, &mut pts_counter);
                }

                let _flags = capture_client.read_from_device_to_deque(&mut total_buffer).unwrap();
                //println!("{:?}", total_buffer);

                while total_buffer.len() >= AAC_FRAME_SIZE * self.frame.format().bytes() * self.frame.channel_layout().channels() as usize {
                    //let buffer: Vec<u8> = total_buffer.drain(..1024 * self.format.nBlockAlign as usize).collect();
                    let buffer: Vec<u8> = total_buffer.drain(..AAC_FRAME_SIZE * self.frame.format().bytes() * self.frame.channel_layout().channels() as usize).collect();
                    let sample_frames = AAC_FRAME_SIZE;//packet_length as usize; //buffer.len() / (*format).nBlockAlign as usize;

                    Self::copy_into_frame(&mut self.frame, buffer);
                    self.frame.set_pts(Some(pts_counter));
                    Self::send_frame_and_receive_packets(&self.ring_buffer, &mut self.audio_encoder, &self.frame, AAC_FRAME_SIZE as i64);

                    pts_counter += sample_frames as i64;
                }
            }
        })
    }
}

impl<P: PacketRingBuffer> AudioPerProcess<P> {
    fn flush_and_silence(&mut self, vec_to_be_flushed: &mut VecDeque<u8>, mut frames_of_silence: i64, start_pts: &mut i64) {
        // flush
        //let mut buffer = vec![0; self.frame.format().bytes()];
        let mut buffer = vec![0; AAC_FRAME_SIZE * self.frame.format().bytes() * self.frame.channel_layout().channels() as usize];
        assert!(buffer.len() >= vec_to_be_flushed.len(), "{}, {}", buffer.len(), vec_to_be_flushed.len());
        let flushed_pts = (buffer.len() - vec_to_be_flushed.len()) / 8; //self.client.get_mixformat().unwrap().wave_fmt.Format.nBlockAlign as usize;
        assert_eq!(flushed_pts * /*self.client.get_mixformat().unwrap().wave_fmt.Format.nBlockAlign as usize*/8, buffer.len() - vec_to_be_flushed.len());

        let (first, second) = vec_to_be_flushed.as_slices();
        assert_eq!(first.len() + second.len(), vec_to_be_flushed.len());
        buffer[..first.len()].copy_from_slice(first);
        buffer[first.len()..vec_to_be_flushed.len()].copy_from_slice(second);
        frames_of_silence -= flushed_pts as i64;

        Self::copy_into_frame(&mut self.frame, buffer);
        self.frame.set_pts(Some(*start_pts));
        Self::send_frame_and_receive_packets(&self.ring_buffer, &mut self.audio_encoder, &self.frame, AAC_FRAME_SIZE as i64);
        *start_pts += AAC_FRAME_SIZE as i64;

        // empty frames
        let whole_silent_frames = frames_of_silence / AAC_FRAME_SIZE as i64;//frames_of_silence & !(1024-1); // frames_of_silence / 1024 * 1024;

        println!("{}, {}", whole_silent_frames, frames_of_silence);
        for _ in 0..whole_silent_frames {
            self.empty_frame.set_pts(Some(*start_pts));
            Self::send_frame_and_receive_packets(&self.ring_buffer, &mut self.audio_encoder, &self.empty_frame, AAC_FRAME_SIZE as i64);
            *start_pts += AAC_FRAME_SIZE as i64;
        }
    }

    fn copy_into_frame(frame: &mut ffmpeg_next::util::frame::audio::Audio, buffer: Vec<u8>) {
        let linesize = unsafe { (*frame.as_ptr()).linesize[0] as usize };
        let ptr0 = unsafe { (*frame.as_ptr()).extended_data.offset(0).read() };
        let ptr1 = unsafe { (*frame.as_ptr()).extended_data.offset(1).read() };
        // Get mutable slices to the destination planes first
        let left_plane = unsafe { std::slice::from_raw_parts_mut(ptr0, linesize) };
        let right_plane = unsafe { std::slice::from_raw_parts_mut(ptr1, linesize) };

        // Process buffer directly into planes
        for (i, chunk) in buffer.chunks(8).enumerate() {
            if chunk.len() >= 8 {
                let offset = i * 4;
                if offset + 4 > left_plane.len() || offset + 4 > right_plane.len() {
                    panic!("Destination planes too small");
                }
                left_plane[offset..offset + 4].copy_from_slice(&chunk[0..4]);
                right_plane[offset..offset + 4].copy_from_slice(&chunk[4..8]);
            } else {
                panic!("Data not divisible by 8");
            }
        }
    }
}

struct MaybeSafeHANDLE(Handle);
unsafe impl Send for MaybeSafeHANDLE {}
unsafe impl Sync for MaybeSafeHANDLE {}
impl std::ops::Deref for MaybeSafeHANDLE {
    type Target = Handle;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl std::ops::DerefMut for MaybeSafeHANDLE {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

struct MaybeSafeAudioClient(AudioClient);
unsafe impl Send for MaybeSafeAudioClient {}
unsafe impl Sync for MaybeSafeAudioClient {}
impl std::ops::Deref for MaybeSafeAudioClient {
    type Target = AudioClient;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl std::ops::DerefMut for MaybeSafeAudioClient {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}