use std::sync::Arc;
use std::sync::atomic::Ordering;

use crate::decoders::FfmpegDecoder;
use crate::stream_scheduler::{PlayState, StreamFrameScheduler};

pub struct StreamHandle {
    play_state: Arc<PlayState>,
}

impl StreamHandle {
    pub fn change_state(&self) {
        self.play_state.flip_playing();
    }

    pub fn clear_buffered_frames(&self) {

    }
}

pub struct Stream<D: FfmpegDecoder> {
    decoder: D,
    pub stream_scheduler: Box<dyn StreamFrameScheduler<D::DecodedFrame>>,
    play_state: Arc<PlayState>,
}

impl<D: FfmpegDecoder> Stream<D> {
    pub fn new(decoder: D, stream_scheduler: Box<dyn StreamFrameScheduler<D::DecodedFrame>>) -> Self {
        let play_state = stream_scheduler.get_play_state();
        Self {
            decoder,
            stream_scheduler,
            play_state,
        }
    }

    pub fn start(&mut self) {
        self.stream_scheduler.start()
    }

    pub fn get_stream_handle(&self) -> StreamHandle {
        StreamHandle {
            play_state: self.play_state.clone(),
        }
    }

    pub async fn process_packet(&mut self, packet: &ffmpeg_next::Packet) {
        let frames = self.decoder.process_packet(packet);
        for frame in frames {
            self.stream_scheduler.insert_frame(frame).await;
        }
    }
}