use ffmpeg_next::{decoder, frame};
use ffmpeg_next::format::{Pixel, Sample};
use ffmpeg_next::software::scaling;
use ffmpeg_next::software::scaling::Flags;

pub trait FfmpegDecoder {
    type DecodedFrame;
    fn process_packet(&mut self, packet: &ffmpeg_next::packet::Packet) -> Vec<Self::DecodedFrame>;
}

pub struct VideoDecoder {
    pub decoder: decoder::Video,
    pub out_width: u32,
    pub out_height: u32,
}
impl FfmpegDecoder for VideoDecoder {
    type DecodedFrame = frame::Video;
    fn process_packet(&mut self, packet: &ffmpeg_next::packet::Packet) -> Vec<Self::DecodedFrame> {
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

            frames.push(rgb_frame);
        }

        frames
    }
}

pub struct AudioDecoder {
    pub decoder: decoder::Audio,
}

impl FfmpegDecoder for AudioDecoder {
    type DecodedFrame = frame::Audio;
    fn process_packet(&mut self, packet: &ffmpeg_next::packet::Packet) -> Vec<Self::DecodedFrame> {
        let mut frames = Vec::new();

        let _ = self.decoder.send_packet(packet);

        let mut audio_frame = frame::Audio::empty();

        while let Ok(_) = self.decoder.receive_frame(&mut audio_frame) {
            let format = audio_frame.format();
            matches!(format, Sample::F32(_));

            frames.push(audio_frame.clone());
        }
        frames
    }
}