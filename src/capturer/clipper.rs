use std::sync::{Arc, Mutex};

use crate::capturer::capturer::Capturer;
use crate::capturer::key_listener::KeyListener;
use crate::capturer::ring_buffer::RingBuffer;
use crate::capturer::saver::Saver;

pub struct Clipper {
    ring_buffer: Arc<Mutex<RingBuffer>>,
    video_encoder: Arc<Mutex<ffmpeg_next::codec::encoder::Video>>,


    capturer: Capturer,
    key_listener: KeyListener,
}

impl Clipper {
    pub fn new(fps: i32, width: u32, height: u32, max_seconds: i32) -> Self {
        let codec = ffmpeg_next::codec::encoder::find(ffmpeg_next::codec::id::Id::H264).or_else(|| {
            println!("H264 not found :(");
            ffmpeg_next::codec::encoder::find_by_name("libx264")
        }).ok_or(ffmpeg_next::Error::EncoderNotFound).unwrap();
        let mut ctx = ffmpeg_next::codec::context::Context::new_with_codec(codec);
        let mut enc = ctx.encoder().video().unwrap();

        // 4. Set parameters
        enc.set_width(width);
        enc.set_height(height);
        enc.set_format(ffmpeg_next::format::Pixel::YUV420P);
        enc.set_time_base((1, fps));
        enc.set_frame_rate(Some((fps, 1)));
        enc.set_bit_rate(8_000_000);

        enc.set_flags(ffmpeg_next::codec::Flags::GLOBAL_HEADER); // ← Ensures extradata is generated
        //enc.set_max_b_frames(0); // ← MP4 typically needs this
        enc.set_gop(fps as u32); // Keyframe interval (1 second)

        // 5. Open it
        let _v = enc.open_as(codec).unwrap();
        let mut video_encoder = Arc::new(Mutex::new(_v));

        let ring_buffer: Arc<Mutex<RingBuffer>> = Arc::new(Mutex::new(RingBuffer::new(fps * max_seconds)));


        let capturer = Capturer::new(fps, width, height, Arc::clone(&video_encoder), Arc::clone(&ring_buffer));
        let saver = Saver::new(Arc::clone(&video_encoder), Arc::clone(&ring_buffer), "out", "Chat Clip That", ".mp4");
        let key_listener = KeyListener::new(saver);

        Self {
            ring_buffer,
            video_encoder,
            capturer,
            key_listener,
        }
    }

    pub fn start(self) {
        let capture_join = self.capturer.start_capturing();
        let listen_join = self.key_listener.start_key_listener();

        capture_join.join().unwrap().unwrap();
        listen_join.join().unwrap();
    }
}