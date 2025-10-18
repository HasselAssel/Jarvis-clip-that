use eframe::egui::ColorImage;
use ffmpeg_next::format::{Pixel, Sample};
use ffmpeg_next::frame;
use ffmpeg_next::software::scaling;
use ffmpeg_next::software::scaling::Flags;
use rodio::buffer::SamplesBuffer;
use std::time::Instant;
use ffmpeg_next::ffi::AVCodecContext;
use crate::hw_decoding::idk_yet;

pub struct DecodedFrame {
    pub video: Option<(Vec<u8>, [usize; 2])>,
    pub audio: Option<SamplesBuffer<f32>>,
}

pub trait Decoder {
    fn process_packet(&mut self, packet: &ffmpeg_next::packet::Packet) -> Vec<DecodedFrame>;
    fn get_codec_ctx(&self) -> *mut AVCodecContext;
}

impl Decoder for ffmpeg_next::decoder::Video {
    fn process_packet(&mut self, packet: &ffmpeg_next::packet::Packet) -> Vec<DecodedFrame> {
        let instant = Instant::now();
        let mut frames = Vec::new();

        let _ = self.send_packet(packet);

        let mut video_frame = frame::Video::empty();

        while let Ok(_) = self.receive_frame(&mut video_frame) {
            let instant1 = Instant::now();
            let mut scaler = scaling::context::Context::get(
                video_frame.format(),          // input format
                video_frame.width(),
                video_frame.height(),
                Pixel::RGB24,            // output format
                video_frame.width(),
                video_frame.height(),
                Flags::FAST_BILINEAR,
            ).unwrap();
            println!("scaler creation: {:?}", instant1.elapsed());


            let mut rgb_frame = frame::Video::empty();
            let instant1 = Instant::now();
            scaler.run(&mut video_frame, &mut rgb_frame).unwrap();
            println!("scaler run: {:?}", instant1.elapsed());


            let instant1 = Instant::now();
            let rgb_vec = rgb_frame.data(0).to_vec();

            println!("frame creation: {:?}", instant1.elapsed());

            frames.push(DecodedFrame { video: Some((rgb_vec, [rgb_frame.width() as usize, rgb_frame.height() as usize])), audio: None });
        }
        println!("video elapsed: {:?}", instant.elapsed());
        frames
    }

    fn get_codec_ctx(&self) -> *mut AVCodecContext {
        unsafe { self.0.0.0.as_ptr() as *mut _ }
    }
}

impl Decoder for ffmpeg_next::decoder::Audio {
    fn process_packet(&mut self, packet: &ffmpeg_next::packet::Packet) -> Vec<DecodedFrame> {
        let instant = Instant::now();
        let mut frames = Vec::new();

        let _ = self.send_packet(packet);

        let mut audio_frame = frame::Audio::empty();

        while let Ok(_) = self.receive_frame(&mut audio_frame) {
            let format = audio_frame.format();
            let channels = audio_frame.channels();
            let rate = audio_frame.rate();
            let nb_samples = audio_frame.samples();

            matches!(format, Sample::F32(_));

            let pcm_data = audio_frame.data(0);

            let samples = unsafe {
                std::slice::from_raw_parts(
                    pcm_data.as_ptr() as *const f32,
                    nb_samples * channels as usize,
                )
            };

            let sample_buffer = SamplesBuffer::new(channels, rate, samples);

            frames.push(DecodedFrame { video: None, audio: Some(sample_buffer) });
        }
        println!("video elapsed: {:?}", instant.elapsed());
        frames
    }

    fn get_codec_ctx(&self) -> *mut AVCodecContext {
        todo!()
    }
}