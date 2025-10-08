use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::thread;
use std::thread::JoinHandle;

use ffmpeg_next::encoder::audio::Encoder;
use ffmpeg_next::util::frame::audio::Audio;
use windows::Win32::Media::Audio::{IAudioCaptureClient, IAudioClient, WAVEFORMATEX};
use windows::Win32::System::Threading::{CreateEventW, INFINITE, WaitForSingleObject};

use crate::recorder::parameters::AudioParams;
use crate::recorder::recorders::frame::copy_into_audio_frame;
use crate::recorder::traits::Recorder;
use crate::ring_buffer::traits::PacketRingBuffer;
use crate::types::Result;
use crate::wrappers::{MaybeSafeHANDLEWrapper, MaybeSafeComWrapper};

pub struct AudioCapturer<P: PacketRingBuffer> {
    ring_buffer: Arc<Mutex<P>>,
    audio_encoder: Encoder,
    audio_params: AudioParams,

    client: MaybeSafeComWrapper<IAudioClient>,
    format: WAVEFORMATEX,

    frame: Audio,
    silent_frame: Audio,

    event: MaybeSafeHANDLEWrapper,
}

impl<PRB: PacketRingBuffer> AudioCapturer<PRB> {
    pub fn new(ring_buffer: Arc<Mutex<PRB>>, audio_encoder: Encoder, audio_params: AudioParams, client: IAudioClient, format: WAVEFORMATEX, frame: Audio, silent_frame: Audio) -> Self {
        let client = MaybeSafeComWrapper(client);

        let event;
        unsafe {
            event = CreateEventW(None, false, false, None).unwrap();
            client.SetEventHandle(event).unwrap();
        }
        let event = MaybeSafeHANDLEWrapper(event);

        Self {
            ring_buffer,
            audio_encoder,
            audio_params,
            client,
            format,
            frame,
            silent_frame,
            event,
        }
    }
}

impl<PRB: PacketRingBuffer + 'static> Recorder<PRB> for AudioCapturer<PRB> {
    fn start_capturing(mut self: Box<Self>) -> JoinHandle<Result<()>> {
        thread::spawn(move || -> Result<()> {
            unsafe {
                let frame_size: usize = self.frame.samples();
                println!("Audio Frame Size: {}", frame_size);
                assert_eq!(frame_size, self.silent_frame.samples(), "'frame' and 'silent_frame' have different sizes!");
                assert_eq!(self.format.nSamplesPerSec as i32, self.audio_params.base_params.rate, "Custom audio sample rate not supported, pls use default: {}, instead of {}", {self.format.nSamplesPerSec}, self.audio_params.base_params.rate);

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
                        if diff >= frame_size as _ {
                            println!("ADDED SILENCE!!!");
                            self.flush_and_silence(&mut total_buffer, diff, &mut pts_counter);
                        }

                        let buffer = std::slice::from_raw_parts(
                            data as *const u8,
                            packet_length as usize * self.format.nBlockAlign as usize,
                        );

                        assert_eq!(buffer.len() % 8, 0);
                        total_buffer.extend(buffer);

                        capture_client.ReleaseBuffer(packet_length)?;
                    }

                    while total_buffer.len() >= frame_size * self.format.nBlockAlign as usize {
                        let buffer: Vec<u8> = total_buffer.drain(..frame_size * self.format.nBlockAlign as usize).collect();
                        let sample_frames = frame_size;


                        unsafe { copy_into_audio_frame(&mut self.frame, buffer); }
                        self.frame.set_pts(Some(pts_counter));
                        Self::send_frame_and_receive_packets(&self.ring_buffer, &mut self.audio_encoder, &self.frame, frame_size as i64).unwrap();

                        pts_counter += sample_frames as i64;
                    }
                }
            }
        })
    }
}

impl<PRB: PacketRingBuffer + 'static> AudioCapturer<PRB> {
    fn flush_and_silence(&mut self, vec_to_be_flushed: &mut VecDeque<u8>, mut frames_of_silence: i64, start_pts: &mut i64) {
        let frame_size: usize = self.frame.samples();
        // flush
        let mut buffer = vec![0; frame_size * self.format.nBlockAlign as usize];
        assert!(buffer.len() >= vec_to_be_flushed.len());
        let flushed_pts = (buffer.len() - vec_to_be_flushed.len()) / self.format.nBlockAlign as usize;
        assert_eq!(flushed_pts * self.format.nBlockAlign as usize, buffer.len() - vec_to_be_flushed.len());

        let (first, second) = vec_to_be_flushed.as_slices();
        assert_eq!(first.len() + second.len(), vec_to_be_flushed.len());
        buffer[..first.len()].copy_from_slice(first);
        buffer[first.len()..vec_to_be_flushed.len()].copy_from_slice(second);
        frames_of_silence -= flushed_pts as i64;

        unsafe { copy_into_audio_frame(&mut self.frame, buffer); }
        self.frame.set_pts(Some(*start_pts));
        Self::send_frame_and_receive_packets(&self.ring_buffer, &mut self.audio_encoder, &self.frame, frame_size as i64).unwrap();
        *start_pts += frame_size as i64;

        // empty frames
        let whole_silent_frames = frames_of_silence / frame_size as i64;

        for _ in 0..whole_silent_frames {
            self.silent_frame.set_pts(Some(*start_pts));
            Self::send_frame_and_receive_packets(&self.ring_buffer, &mut self.audio_encoder, &self.silent_frame, frame_size as i64).unwrap();
            *start_pts += frame_size as i64;
        }
    }
}