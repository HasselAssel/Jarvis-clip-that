use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use crate::decoders::FfmpegDecoder;
use crate::stream_scheduler::StreamFrameScheduler;

struct StreamHandle {
    is_playing: Arc<AtomicBool>
}

pub struct Stream<D: FfmpegDecoder, S: StreamFrameScheduler<D::DecodedFrame>> {
    decoder: D,
    stream_scheduler: S,
    is_playing: Arc<AtomicBool>,
}

impl<D: FfmpegDecoder, S: StreamFrameScheduler<D::DecodedFrame>> Stream<D, S> {
    pub fn new(decoder: D, stream_scheduler: S) -> Self {
        Self {
            decoder,
            stream_scheduler,
            is_playing: Arc::new(AtomicBool::new(true)),
        }
    }

    pub fn get_stream_handle(&self) -> StreamHandle {
        StreamHandle {
            is_playing: self.is_playing.clone(),
        }
    }

    pub fn process_packet(&mut self, packet: &ffmpeg_next::Packet) {
        let frames = self.decoder.process_packet(packet);
        for frame in frames {
            self.stream_scheduler.insert_frame(frame);
        }
    }
}