use std::sync::{Arc, mpsc, Mutex};
use std::sync::mpsc::Receiver;

use ffmpeg_next::Packet;
use rdev::Key;

use crate::capturer::capture::audio_per_process::_AudioPerProcess;
use crate::capturer::capture::main_audio_capturer::AudioCapturer;
use crate::capturer::capture::main_video_capturer::VideoCapturer;
use crate::capturer::capture::recorder::RecorderCarrier;
use crate::capturer::clipping::key_listener::KeyListener;
use crate::capturer::clipping::saver::Saver;
use crate::capturer::ring_buffer::{KeyFrameStartPacketWrapper, RingBuffer};

pub struct MainCapturer {
    video_capturer: RecorderCarrier<RingBuffer<KeyFrameStartPacketWrapper>, VideoCapturer<RingBuffer<KeyFrameStartPacketWrapper>>>,
    //audio_capturer: RecorderCarrier<RingBuffer<Packet>, AudioCapturer<RingBuffer<Packet>>>,
    audio_capturer: RecorderCarrier<RingBuffer<Packet>, _AudioPerProcess<RingBuffer<Packet>>>,
    saver: Saver,

    key_listener: KeyListener,
    receiver: Receiver<()>,
}

impl MainCapturer {
    pub fn new() -> Self {
        let one_vid_buf = Arc::new(Mutex::new(RingBuffer::new(30 * 10)));
        let one_aud_buf = Arc::new(Mutex::new(RingBuffer::new(48_000 * 10)));
        let (vid_cap, parv) = VideoCapturer::new(one_vid_buf.clone(), 0).unwrap();
        //let (aud_cap, para) = AudioCapturer::new(one_aud_buf.clone());

        let mut input = String::new();
        std::io::stdin().read_line(&mut input).expect("Failed to read line");

        let trimmed = input.trim();
        match trimmed.parse::<u32>() {
            Ok(num) => println!("You entered: {}", num),
            Err(_) => println!("Invalid number!"),
        }
        let num = trimmed.parse::<u32>().unwrap();

        let (aud_cap, para) = _AudioPerProcess::new(num, true, one_aud_buf.clone()).unwrap();
        let one_vid = RecorderCarrier::new(one_vid_buf.clone(), vid_cap);
        let one_aud = RecorderCarrier::new(one_aud_buf.clone(), aud_cap);
        let saver = Saver::new(parv, para, "out", "Chat Clip That", ".mp4");

        let (sender, receiver) = mpsc::channel::<()>();
        let mut key_listener = KeyListener::new();
        key_listener.register_shortcut(&[Key::Alt, Key::KeyM], move || sender.send(()).unwrap());

        Self {
            video_capturer: one_vid,
            audio_capturer: one_aud,
            saver,
            key_listener,
            receiver,
        }
    }

    pub fn start_capturing(mut self) {
        self.video_capturer.start_capturing().unwrap();
        self.audio_capturer.start_capturing().unwrap();
        self.key_listener.start();

        while self.receiver.recv().is_ok() {
            self.saver.standard_save_to_disc(&self.video_capturer, &self.audio_capturer, None).unwrap()
        }
    }
}