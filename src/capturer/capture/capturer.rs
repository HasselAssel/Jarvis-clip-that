use crate::capturer::capture::audio_capturer::AudioCapturer;
use ffmpeg_next::codec::Flags;
use ffmpeg_next::Packet;
use rdev::Key;
use std::sync::mpsc::Receiver;
use std::sync::{mpsc, Arc, Mutex};

use crate::capturer::capture::recorder::{AudioParams, BaseParams, RecorderCarrier, VideoParams};
use crate::capturer::capture::video_capturer::VideoCapturer;
use crate::capturer::clipping::key_listener::KeyListener;
use crate::capturer::clipping::saver::Saver;
use crate::capturer::ring_buffer::{KeyFrameStartPacketWrapper, RingBuffer};

pub struct Capturer {
    video_capturers: Vec<RecorderCarrier<RingBuffer<KeyFrameStartPacketWrapper>, VideoCapturer<RingBuffer<KeyFrameStartPacketWrapper>>>>,
    audio_capturers: Vec<RecorderCarrier<RingBuffer<Packet>, AudioCapturer<RingBuffer<Packet>>>>,
    saver: Saver,

    key_listener: KeyListener,
    receiver: Receiver<()>,
}

impl Capturer {
    pub fn new() -> Self {

        let video_params = VideoParams{
            base_params: BaseParams {
                //codec: ffmpeg_next::codec::encoder::find_by_name("hevc_amf").ok_or(ffmpeg_next::Error::EncoderNotFound).unwrap(),
                codec: ffmpeg_next::codec::encoder::find_by_name("hevc_qsv").ok_or(ffmpeg_next::Error::EncoderNotFound).unwrap(),
                bit_rate: 8_000_000,
                max_bit_rate: 10_000_000,
                flags: Flags::GLOBAL_HEADER,
                rate: 30, //fps
            },
            out_width: 1500,
            out_height: 1000,
            //format: ffmpeg_next::format::Pixel::D3D11,
            format: ffmpeg_next::format::Pixel::NV12,
        };
        let audio_params = AudioParams{
            base_params: BaseParams {
                codec: ffmpeg_next::codec::encoder::find(ffmpeg_next::codec::Id::AAC).ok_or(ffmpeg_next::Error::EncoderNotFound).unwrap(),
                bit_rate: 128_000,
                max_bit_rate: 150_000,
                flags: Flags::GLOBAL_HEADER,
                rate: 48_000,// TODO has to always match windows!!! 48_000 is just temporary
            },
            channel_layout: ffmpeg_next::util::channel_layout::ChannelLayout::STEREO,
            format: ffmpeg_next::format::Sample::F32(ffmpeg_next::util::format::sample::Type::Planar),
        };

        let one_vid_buf = Arc::new(Mutex::new(RingBuffer::new(1000)));
        let one_aud_buf = Arc::new(Mutex::new(RingBuffer::new(1000)));
        let vid_cap = VideoCapturer::new(one_vid_buf.clone(), &video_params);
        let aud_cap = AudioCapturer::new(one_aud_buf.clone(), &audio_params);
        let one_vid = RecorderCarrier::new(one_vid_buf.clone(), vid_cap);
        let one_aud = RecorderCarrier::new(one_aud_buf.clone(), aud_cap);
        let saver = Saver::new(&video_params, &audio_params, "out", "Chat Clip That", ".mp4");

        let (sender, receiver) = mpsc::channel::<()>();
        let mut key_listener = KeyListener::new();
        key_listener.register_shortcut(&[Key::Alt, Key::KeyN], move || sender.send(()).unwrap());

        Self {
            video_capturers: vec![one_vid],
            audio_capturers: vec![one_aud],
            saver,
            key_listener,
            receiver,
        }
    }

    pub fn start_capturing(mut self) {
        self.video_capturers[0].start_capturing().unwrap();
        self.audio_capturers[0].start_capturing().unwrap();
        self.key_listener.start();

        while self.receiver.recv().is_ok() {
            self.saver.standard_save(&self.video_capturers[0], &self.audio_capturers[0], None).unwrap()
        }
    }
}