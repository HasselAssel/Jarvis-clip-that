use std::time::Instant;

use ffmpeg_next::format::{Pixel, Sample};
use ffmpeg_next::{decoder, frame};
use ffmpeg_next::software::scaling;
use ffmpeg_next::software::scaling::Flags;
use rodio::buffer::SamplesBuffer;

pub enum DecodedFrame {
    Video(frame::Video),
    Audio(frame::Audio),
}

pub trait FfmpegDecoder {
    fn process_packet(&mut self, packet: &ffmpeg_next::packet::Packet) -> Vec<DecodedFrame>;
}

pub struct VideoDecoder {
    pub decoder: decoder::Video,
    pub out_width: u32,
    pub out_height: u32,
}
impl FfmpegDecoder for VideoDecoder {
    fn process_packet(&mut self, packet: &ffmpeg_next::packet::Packet) -> Vec<DecodedFrame> {
        let mut frames = Vec::new();

        let _ = self.decoder.send_packet(packet);

        let mut video_frame = frame::Video::empty();

        while let Ok(_) = self.decoder.receive_frame(&mut video_frame) {
            let mut scaler = scaling::context::Context::get(
                video_frame.format(),
                video_frame.width(),
                video_frame.height(),
                Pixel::RGBA,
                self.out_width,
                self.out_height,
                Flags::FAST_BILINEAR,
            ).unwrap();


            let mut rgb_frame = frame::Video::empty();
            scaler.run(&mut video_frame, &mut rgb_frame).unwrap();

            frames.push(DecodedFrame::Video(rgb_frame));
        }

        frames
    }
}

pub struct AudioDecoder {
    pub decoder: decoder::Audio,
}

impl FfmpegDecoder for AudioDecoder {
    fn process_packet(&mut self, packet: &ffmpeg_next::packet::Packet) -> Vec<DecodedFrame> {
        let mut frames = Vec::new();

        let _ = self.decoder.send_packet(packet);

        let mut audio_frame = frame::Audio::empty();

        while let Ok(_) = self.decoder.receive_frame(&mut audio_frame) {
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

            frames.push(DecodedFrame::Audio(audio_frame.clone()));
        }
        Vec::new()//frames
    }
}