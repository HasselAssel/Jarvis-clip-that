use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;

use ffmpeg_next::encoder::audio::Encoder;
use ffmpeg_next::util::frame::audio::Audio;

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
    pub fn new(
        ring_buffer: Arc<Mutex<PRB>>,
        audio_source: AS,
        audio_encoder: Encoder,
        frame: Audio,
        silent_frame: Audio,
    ) -> Self {
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
    fn start_capturing(
        mut self: Box<Self>,
        stop_capturing_callback: Option<Arc<AtomicBool>>,
    ) -> RecorderJoinHandle {
        fn help<PRB: PacketRingBuffer, AS: AudioSource + Send>(selbst: &mut Box<AudioRecorder<PRB, AS>>) {
            selbst.audio_source.await_new_audio();

            selbst.audio_source.gather_new_audio(&selbst.ring_buffer, &mut selbst.audio_encoder, &mut selbst.frame, &mut selbst.silent_frame).unwrap_or_else(|err| panic!("AudioRecorder: Failed to send_frame_and_receive_packets because: {:?}", err));
        }

        thread::spawn(move || -> Result<()> {
            self.audio_source.init().unwrap_or_else(|err| panic!("Failed to init VideoRecorder: {:?}", err));

            if let Some(stop_capturing_callback) = stop_capturing_callback {
                while stop_capturing_callback.load(Ordering::Relaxed) {
                    help(&mut self);
                }
                Ok(())
            } else {
                loop {
                    help(&mut self);
                }
            }
        })
    }
}