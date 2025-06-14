use std::collections::VecDeque;
use ffmpeg_next::codec::encoder::Audio;
use std::sync::{Arc, Mutex};
use std::thread;
use std::thread::JoinHandle;
use ffmpeg_next::codec::{Flags, Parameters};
use windows::Win32::Foundation::HANDLE;
use windows::Win32::Media::Audio::{eConsole, eRender, IAudioCaptureClient, IAudioClient, IMMDeviceEnumerator, MMDeviceEnumerator, AUDCLNT_SHAREMODE_SHARED, AUDCLNT_STREAMFLAGS_EVENTCALLBACK, AUDCLNT_STREAMFLAGS_LOOPBACK, WAVEFORMATEX};
use windows::Win32::System::Com::CoCreateInstance;
use windows::Win32::System::Threading::{CreateEventW, WaitForSingleObject, INFINITE};
use crate::recorder::parameters::{AudioParams, BaseParams};
use crate::wrappers::UnsafeComWrapper;
use crate::ring_buffer::traits::PacketRingBuffer;

const AAC_FRAME_SIZE: usize = 1024;
pub struct AudioCapturer<P: PacketRingBuffer + 'static> {
    audio_encoder: Audio,
    ring_buffer: Arc<Mutex<P>>,

    client: UnsafeComWrapper<IAudioClient>,
    format: WAVEFORMATEX,

    frame: ffmpeg_next::util::frame::audio::Audio,
    empty_frame: ffmpeg_next::util::frame::audio::Audio,

    event: MaybeSafeHANDLE,
}

impl<P: PacketRingBuffer> AudioCapturer<P> {
    pub fn new(ring_buffer: Arc<Mutex<P>>) -> (Self, Parameters) {
        unsafe {
            windows::Win32::System::Com::CoInitializeEx(None, windows::Win32::System::Com::COINIT_MULTITHREADED).unwrap();
        }

        let enumerator: IMMDeviceEnumerator = unsafe {
            CoCreateInstance(
                &MMDeviceEnumerator,
                None,
                windows::Win32::System::Com::CLSCTX_ALL,
            ).unwrap()
        };

        let device = unsafe {
            enumerator.GetDefaultAudioEndpoint(
                eRender,
                eConsole,
            ).unwrap()
        };

        let client: IAudioClient = unsafe {
            device.Activate(
                windows::Win32::System::Com::CLSCTX_ALL,
                None,
            ).unwrap()
        };

        let format = unsafe { client.GetMixFormat().unwrap() };

        unsafe {
            client.Initialize(
                AUDCLNT_SHAREMODE_SHARED,
                AUDCLNT_STREAMFLAGS_LOOPBACK | AUDCLNT_STREAMFLAGS_EVENTCALLBACK,
                10_000_000,
                0,
                format,
                None,
            ).unwrap();
        }
        let format = unsafe { *format };

        let event;
        unsafe {
            event = CreateEventW(None, false, false, None).unwrap();
            client.SetEventHandle(event).unwrap();
        }
        let event = MaybeSafeHANDLE(event);

        let client = UnsafeComWrapper(client);


        let audio_params = AudioParams{
            base_params: BaseParams {
                codec: ffmpeg_next::codec::encoder::find(ffmpeg_next::codec::Id::AAC).ok_or(ffmpeg_next::Error::EncoderNotFound).unwrap(),
                bit_rate: 128_000,
                max_bit_rate: 150_000,
                flags: Flags::GLOBAL_HEADER,
                rate: format.nSamplesPerSec as i32,
            },
            channel_layout: ffmpeg_next::util::channel_layout::ChannelLayout::STEREO,
            format: ffmpeg_next::format::Sample::F32(ffmpeg_next::util::format::sample::Type::Planar),
        };


        //let codec = ffmpeg_next::codec::encoder::find(ffmpeg_next::codec::Id::AAC).ok_or("AAC encoder not found").unwrap();
        let codec = audio_params.base_params.codec;
        let ctx = ffmpeg_next::codec::context::Context::new_with_codec(codec);
        let mut enc = ctx.encoder().audio().unwrap();

        enc.set_rate(audio_params.base_params.rate); //enc.set_rate(format.nSamplesPerSec as i32);
        enc.set_channel_layout(audio_params.channel_layout); //enc.set_channel_layout(ffmpeg_next::util::channel_layout::ChannelLayout::STEREO);
        enc.set_format(audio_params.format); //enc.set_format(ffmpeg_next::format::Sample::F32(ffmpeg_next::util::format::sample::Type::Planar));
        //enc.set_bit_rate(audio_params.base_params.bit_rate);//enc.set_bit_rate(128_000);
        //enc.set_max_bit_rate(audio_params.base_params.max_bit_rate);//enc.set_max_bit_rate(200_000);
        enc.set_time_base((1, audio_params.base_params.rate)); //enc.set_time_base((1, format.nSamplesPerSec as i32));
        enc.set_flags(audio_params.base_params.flags); //enc.set_flags(Flags::GLOBAL_HEADER);

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
        //let buf = vec![0u8; AAC_FRAME_SIZE * empty_frame.format().bytes() * empty_frame.channel_layout().channels() as usize];
        let buf = vec![0u8; AAC_FRAME_SIZE * format.nBlockAlign as usize];
        Self::copy_into_frame(&mut empty_frame, buf);


        (Self {
            audio_encoder,
            ring_buffer,

            client,
            format,

            frame,
            empty_frame,
            event
        }, par)
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

    fn flush_and_silence(&mut self, vec_to_be_flushed: &mut VecDeque<u8>, mut frames_of_silence: i64, start_pts: &mut i64) {
        // flush
        let mut buffer = vec![0; AAC_FRAME_SIZE * self.format.nBlockAlign as usize];
        assert!(buffer.len() >= vec_to_be_flushed.len());
        let flushed_pts = (buffer.len() - vec_to_be_flushed.len()) / self.format.nBlockAlign as usize;
        assert_eq!(flushed_pts * self.format.nBlockAlign as usize, buffer.len() - vec_to_be_flushed.len());

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
        let whole_silent_frames = frames_of_silence / AAC_FRAME_SIZE as i64;

        for _ in 0..whole_silent_frames {
            self.empty_frame.set_pts(Some(*start_pts));
            Self::send_frame_and_receive_packets(&self.ring_buffer, &mut self.audio_encoder, &self.empty_frame, AAC_FRAME_SIZE as i64);
            *start_pts += AAC_FRAME_SIZE as i64;
        }
    }
}

impl<P: PacketRingBuffer> Recorder<P> for AudioCapturer<P> {
    fn start_capturing(mut self) -> JoinHandle<Result<(), IdkCustomErrorIGuess>> {
        thread::spawn(move || -> Result<(), IdkCustomErrorIGuess> {
            unsafe {
                let mut pts_counter: i64 = 0;
                let mut total_buffer = VecDeque::new();

                let capture_client: IAudioCaptureClient = self.client.GetService()?;

                self.client.Start()?;

                let mut freq = 0;
                let mut start_time = 0;
                windows::Win32::System::Performance::QueryPerformanceFrequency(&mut freq)?;
                windows::Win32::System::Performance::QueryPerformanceCounter(&mut start_time)?;

                loop {
                    WaitForSingleObject(self.event.0, INFINITE);

                    let mut packet_length = 0;
                    let mut data = std::ptr::null_mut();
                    let mut flags = 0;

                    let mut device_pos = 0;
                    let mut qpc_pos = 0;
                    capture_client.GetBuffer(
                        &mut data,
                        &mut packet_length,
                        &mut flags,
                        Some(&mut device_pos),
                        Some(&mut qpc_pos),
                    )?;

                    if packet_length > 0 {
                        let new_pts = ((qpc_pos - start_time as u64) * self.format.nSamplesPerSec as u64 / freq as u64) as i64;
                        let diff = (new_pts - pts_counter).max(0);
                        if diff >= AAC_FRAME_SIZE as _ {
                            println!("ADDED SILENCE!!!");
                            self.flush_and_silence(&mut total_buffer, diff, &mut pts_counter);
                        }


                        let buffer = std::slice::from_raw_parts(
                            data as *const u8,
                            packet_length as usize * self.format.nBlockAlign as usize,
                        );

                        assert_eq!(buffer.len() % 8, 0);
                        //total_buffer.extend_from_slice(buffer);
                        total_buffer.extend(buffer);

                        capture_client.ReleaseBuffer(packet_length)?;
                    }

                    while total_buffer.len() >= AAC_FRAME_SIZE * self.format.nBlockAlign as usize {
                        //let buffer: Vec<u8> = total_buffer.drain(..1024 * self.format.nBlockAlign as usize).collect();
                        let buffer: Vec<u8> = total_buffer.drain(..AAC_FRAME_SIZE * self.format.nBlockAlign as usize).collect();
                        let sample_frames = AAC_FRAME_SIZE;//packet_length as usize; //buffer.len() / (*format).nBlockAlign as usize;

                        Self::copy_into_frame(&mut self.frame, buffer);
                        self.frame.set_pts(Some(pts_counter));
                        Self::send_frame_and_receive_packets(&self.ring_buffer, &mut self.audio_encoder, &self.frame, AAC_FRAME_SIZE as i64);

                        pts_counter += sample_frames as i64;
                    }
                }
            }
            // Ok(())
        })
    }
}

struct MaybeSafeHANDLE(HANDLE);

unsafe impl Send for MaybeSafeHANDLE {}
unsafe impl Sync for MaybeSafeHANDLE {}