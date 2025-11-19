use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use crate::decoders::FfmpegDecoder;
use crate::stream_scheduler::StreamFrameScheduler;

pub struct StreamHandle {
    is_playing: Arc<AtomicBool>,
}

pub struct Stream<D: FfmpegDecoder> {
    decoder: D,
    pub stream_scheduler: Box<dyn StreamFrameScheduler<D::DecodedFrame>>,
    is_playing: Arc<AtomicBool>,
}

impl<D: FfmpegDecoder> Stream<D> {
    pub fn new(decoder: D, stream_scheduler: Box<dyn StreamFrameScheduler<D::DecodedFrame>>) -> Self {
        Self {
            decoder,
            stream_scheduler,
            is_playing: Arc::new(AtomicBool::new(true)),
        }
    }

    pub fn start(&mut self) {
        self.stream_scheduler.start()
    }

    pub fn get_stream_handle(&self) -> StreamHandle {
        StreamHandle {
            is_playing: self.is_playing.clone(),
        }
    }

    pub async fn process_packet(&mut self, packet: &ffmpeg_next::Packet) {
        let frames = self.decoder.process_packet(packet);
        for frame in frames {
            self.stream_scheduler.insert_frame(frame).await;
        }
    }
}