use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::thread;

use ffmpeg_next::encoder::audio::Encoder;
use ffmpeg_next::util::frame::audio::Audio;
use windows::Win32::System::Performance::{QueryPerformanceCounter, QueryPerformanceFrequency};

use crate::recorders::audio::sources::traits::AudioSource;
use crate::recorders::traits::TRecorder;
use crate::ring_buffer::traits::PacketRingBuffer;
use crate::types::{RecorderJoinHandle, Result};

pub struct AudioRecorder<PRB: PacketRingBuffer + 'static, AS: AudioSource + Send + 'static> {
    ring_buffer: Arc<Mutex<PRB>>,
    audio_source: AS,

    audio_encoder: Encoder,
    frame: Audio,
    silent_frame: Audio,
}

impl<PRB: PacketRingBuffer, AS: AudioSource + Send> AudioRecorder<PRB, AS> {
    pub fn new(ring_buffer: Arc<Mutex<PRB>>, audio_source: AS, audio_encoder: Encoder, frame: Audio, silent_frame: Audio) -> Self {
        Self {
            ring_buffer,
            audio_source,

            audio_encoder,
            frame,
            silent_frame,
        }
    }
}

impl<PRB: PacketRingBuffer, AS: AudioSource + Send> TRecorder<PRB> for AudioRecorder<PRB, AS> {
    fn start_capturing(mut self: Box<Self>) -> RecorderJoinHandle {
        thread::spawn(move || -> Result<()> {
            self.audio_source.init();

            loop {
                self.audio_source.await_new_audio();

                self.audio_source.gather_new_audio(&self.ring_buffer, &mut self.audio_encoder, &mut self.frame, &mut self.silent_frame).expect("TODO: panic message");
            }
        })
    }
}