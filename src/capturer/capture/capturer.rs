use std::sync::{Arc, Mutex};
use ffmpeg_next::Packet;
use crate::capturer::capture::audio_capturer::AudioCapturer;

use crate::capturer::capture::recorder::RecorderCarrier;
use crate::capturer::capture::video_capturer::VideoCapturer;
use crate::capturer::clipping::key_listener::KeyListener;
use crate::capturer::clipping::saver::Saver;
use crate::capturer::ring_buffer::{KeyFrameStartPacketWrapper, RingBuffer};

struct Capturer {
    video_capturers: Vec<RecorderCarrier<RingBuffer<KeyFrameStartPacketWrapper>, VideoCapturer<RingBuffer<KeyFrameStartPacketWrapper>>>>,
    audio_capturers: Vec<RecorderCarrier<RingBuffer<Packet>, VideoCapturer<RingBuffer<Packet>>>>,

    saver: Saver,
    key_listener: KeyListener,
}

impl Capturer {
    pub fn new() -> Self {
        let one_vid_buf = Arc::new(Mutex::new(RingBuffer::new(1000)));
        let one_aud_buf = Arc::new(Mutex::new(RingBuffer::new(1000)));
        let (vid_cap, vid_enc) = VideoCapturer::new(one_vid_buf, 30, 1500, 1000);
        let (aud_cap, aud_enc) = AudioCapturer::new(one_aud_buf);
        let one_vid = RecorderCarrier::new(Arc::clone(&one_vid_buf), vid_cap);
        let one_aud = RecorderCarrier::new(Arc::clone(&one_aud_buf), aud_cap);
        let mut saver = Saver::new( "out", "Chat Clip That", ".mp4");
        let key_listener = KeyListener::new(|| saver.standard_save(None).ok().unwrap());
        Self {
            video_capturers: vec![one_vid],
            audio_capturers: vec![one_aud],
            saver,
            key_listener,
        }
    }

    fn start_capturing(self) {

    }
}