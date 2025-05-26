use std::sync::{Arc, Mutex};
use std::thread;
use std::thread::JoinHandle;
use ffmpeg_next::codec::{Flags, Parameters};
use ffmpeg_next::frame::Audio;
use windows::Win32::Foundation::HANDLE;
use windows::Win32::Media::Audio::{AUDCLNT_SHAREMODE_SHARED, AUDCLNT_STREAMFLAGS_EVENTCALLBACK, AUDCLNT_STREAMFLAGS_LOOPBACK, eConsole, eRender, IAudioCaptureClient, IAudioClient, IMMDeviceEnumerator, MMDeviceEnumerator, WAVEFORMATEX};
use windows::Win32::System::Com::CoCreateInstance;
use windows::Win32::System::Threading::{CreateEventW, INFINITE, WaitForSingleObject};
use crate::capturer::capture::recorder::Recorder;
use crate::capturer::error::IdkCustomErrorIGuess;
use crate::capturer::ring_buffer::PacketRingBuffer;
use crate::com::ComObj;

pub struct AudioCapturer<P: PacketRingBuffer + 'static> {
    audio_encoder: Arc<Mutex<ffmpeg_next::codec::encoder::Audio>>,
    ring_buffer: Arc<Mutex<P>>,

    client: ComObj<IAudioClient>,
    format: WAVEFORMATEX,

    frame: Audio,
    empty_frame: Audio,

    event: MaybeSafeHANDLE,
}

impl<P: PacketRingBuffer> AudioCapturer<P> {
    pub fn new(ring_buffer: Arc<Mutex<P>>) -> (Self, Arc<Mutex<ffmpeg_next::codec::encoder::Audio>>) { // Todo: Make the construction more general!
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
                10000000,
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

        let client = ComObj(client);


        let codec = ffmpeg_next::codec::encoder::find(ffmpeg_next::codec::Id::AAC)
            .ok_or("AAC encoder not found").unwrap();
        let ctx = ffmpeg_next::codec::context::Context::new_with_codec(codec);
        let mut enc = ctx.encoder().audio().unwrap();

        enc.set_rate(format.nSamplesPerSec as i32);
        enc.set_channel_layout(ffmpeg_next::util::channel_layout::ChannelLayout::STEREO);
        enc.set_format(ffmpeg_next::format::Sample::F32(ffmpeg_next::util::format::sample::Type::Planar));
        //enc.set_bit_rate(128_000);
        enc.set_time_base((1, format.nSamplesPerSec as i32));
        enc.set_flags(Flags::GLOBAL_HEADER);

        let audio_encoder = enc.open_as(codec).unwrap();

        let mut frame = Audio::new(
            audio_encoder.format(),
            1024,
            audio_encoder.channel_layout(),
        );
        let mut empty_frame = Audio::new(
            audio_encoder.format(),
            1024,
            audio_encoder.channel_layout(),
        );
        Self::copy_into_frame(&mut empty_frame, vec![0u8; 1024 * format.nBlockAlign as usize]);

        let audio_encoder = Arc::new(Mutex::new(audio_encoder));
        let aret = Arc::clone(&audio_encoder);

        (Self {
            audio_encoder,
            ring_buffer,

            client,
            format,

            frame,
            empty_frame,
            event
        }, aret)
    }

    fn copy_into_frame(frame: &mut Audio, buffer: Vec<u8>) {
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

    fn flush_and_silence(&mut self, vec_to_be_flushed: &mut Vec<u8>, mut frames_of_silence: i64, start_pts: &mut i64) {
        // flush
        let mut buffer = vec![0; 1024 * self.format.nBlockAlign as usize];
        assert!(buffer.len() > vec_to_be_flushed.len());
        println!("vec_to_be_flushed.len(): {}", vec_to_be_flushed.len());
        let flushed_pts = (buffer.len() - vec_to_be_flushed.len()) / self.format.nBlockAlign as usize;
        assert_eq!(flushed_pts * self.format.nBlockAlign as usize, buffer.len() - vec_to_be_flushed.len());
        buffer[..vec_to_be_flushed.len()].copy_from_slice(vec_to_be_flushed);
        frames_of_silence -= flushed_pts as i64;
        Self::copy_into_frame(&mut self.frame, buffer);
        self.frame.set_pts(Some(*start_pts));
        let mut audio_encoder = self.audio_encoder.lock().unwrap();
        Self::send_frame_and_receive_packets(&self.ring_buffer, &mut audio_encoder, &self.frame);
        *start_pts += 1024;

        // empty frames
        let whole_silent_frames = frames_of_silence / 1024;//frames_of_silence & !(1024-1); // frames_of_silence / 1024 * 1024;
        let mut i = 0;
        println!("{}, {}", whole_silent_frames, frames_of_silence);
        for _ in 0..whole_silent_frames {
            self.empty_frame.set_pts(Some(*start_pts));
            Self::send_frame_and_receive_packets(&self.ring_buffer, &mut audio_encoder, &self.empty_frame);
            *start_pts += 1024;
            i += 1;
        }
        drop(audio_encoder);
        println!("he done!");
    }
}

impl<P: PacketRingBuffer> Recorder<P> for AudioCapturer<P> {
    fn start_capturing(mut self) -> JoinHandle<Result<(), IdkCustomErrorIGuess>> {
        thread::spawn(move || -> Result<(), IdkCustomErrorIGuess> {
            unsafe {
                let mut pts_counter: i64 = 0;
                let mut total_buffer = Vec::new();

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
                        let new_pts = ((qpc_pos - start_time as u64) * 48000u64 / freq as u64) as i64;
                        let diff = (new_pts - pts_counter).max(0);
                        if diff >= 1024 {
                            println!("ADDED SILENCE!!!");
                            self.flush_and_silence(&mut total_buffer, diff, &mut pts_counter);
                        }


                        let buffer = std::slice::from_raw_parts(
                            data as *const u8,
                            packet_length as usize * self.format.nBlockAlign as usize,
                        );

                        assert_eq!(buffer.len() % 8, 0);
                        total_buffer.extend_from_slice(buffer);

                        capture_client.ReleaseBuffer(packet_length)?;
                    }

                    while total_buffer.len() >= 1024 * self.format.nBlockAlign as usize {
                        let buffer: Vec<u8> = total_buffer.drain(..1024 * self.format.nBlockAlign as usize).collect();
                        let sample_frames = 1024;//packet_length as usize; //buffer.len() / (*format).nBlockAlign as usize;

                        Self::copy_into_frame(&mut self.frame, buffer);
                        self.frame.set_pts(Some(pts_counter));
                        let mut audio_encoder = self.audio_encoder.lock().unwrap();
                        Self::send_frame_and_receive_packets(&self.ring_buffer, &mut audio_encoder, &self.frame);
                        drop(audio_encoder);

                        pts_counter += sample_frames as i64;
                    }
                }
            }
            Ok(())
        })
    }
}

pub struct MaybeSafeHANDLE(HANDLE);

unsafe impl Send for MaybeSafeHANDLE {}
unsafe impl Sync for MaybeSafeHANDLE {}