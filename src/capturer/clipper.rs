use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use rand::TryRngCore;
use crate::capturer::audio_capturer::AudioCapturer;

use crate::capturer::capturer::Capturer;
use crate::capturer::key_listener::KeyListener;
use crate::capturer::ring_buffer::RingBuffer;
use crate::capturer::saver::Saver;

pub struct Clipper {
    video_ring_buffer: Arc<Mutex<RingBuffer>>,
    video_encoder: Arc<Mutex<ffmpeg_next::codec::encoder::Video>>,
    /*audio_ring_buffer: Arc<Mutex<RingBuffer>>,
    audio_encoder: Arc<Mutex<ffmpeg_next::codec::encoder::Audio>>,


    audio_capturer: AudioCapturer,*/
    capturer: Capturer,
    key_listener: KeyListener,
}

impl Clipper {
    pub fn new(fps: i32, width: u32, height: u32, max_seconds: i32) -> Self {
        let video_ring_buffer: Arc<Mutex<RingBuffer>> = Arc::new(Mutex::new(RingBuffer::new(fps * max_seconds)));
        let audio_ring_buffer: Arc<Mutex<RingBuffer>> = Arc::new(Mutex::new(RingBuffer::new(44_100 * max_seconds)));


        let (capturer, video_encoder) = Capturer::new(fps, width, height, Arc::clone(&video_ring_buffer));
        //let (audio_capturer, audio_encoder) = AudioCapturer::new(fps,Arc::clone(&audio_ring_buffer));
        let saver = Saver::new(Arc::clone(&video_encoder), Arc::clone(&video_ring_buffer), /*Arc::clone(&audio_encoder), Arc::clone(&audio_ring_buffer),*/ "out", "Chat Clip That", ".mp4");
        let key_listener = KeyListener::new(saver);

        Self {
            video_ring_buffer,
            video_encoder,

            /*audio_ring_buffer,
            audio_encoder,


            audio_capturer,*/
            capturer,
            key_listener,
        }
    }

    pub fn start(self) {
        let capture_join = self.capturer.start_capturing();
        let listen_join = self.key_listener.start_key_listener();
        //let audio_join = self.audio_capturer.start_capturing();

        capture_join.join().unwrap().unwrap();
        listen_join.join().unwrap();
        //audio_join.join().unwrap().unwrap();
    }
}