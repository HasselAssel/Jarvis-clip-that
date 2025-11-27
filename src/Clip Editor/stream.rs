use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::decoders::FfmpegDecoder;
use crate::stream_scheduler::{PlayState, StreamFrameScheduler};

pub struct StreamHandle {
    play_state: Arc<PlayState>,
    request_new_channel: Arc<AtomicBool>,
}

impl StreamHandle {
    pub fn change_state(&self) {
        self.play_state.flip_playing();
    }

    pub fn set_state(&self, state: bool) {
        self.play_state.set_playing(state)
    }

    pub fn clear_buffered_frames(&self) {
        eprintln!("stop!");
        self.request_new_channel.store(true, Ordering::SeqCst)
    }
}

pub struct Stream<D: FfmpegDecoder> {
    decoder: D,
    pub stream_scheduler: Box<dyn StreamFrameScheduler<D::DecodedFrame>>,
    play_state: Arc<PlayState>,
    request_new_channel: Arc<AtomicBool>,
}

impl<D: FfmpegDecoder> Stream<D> {
    pub fn new(decoder: D, stream_scheduler: Box<dyn StreamFrameScheduler<D::DecodedFrame>>) -> Self {
        let play_state = stream_scheduler.get_play_state();
        let request_new_channel = stream_scheduler.get_request_new_channel();
        Self {
            decoder,
            stream_scheduler,
            play_state,
            request_new_channel,
        }
    }

    pub fn start(&mut self) -> bool {
        self.stream_scheduler.start()
    }

    pub fn get_stream_handle(&self) -> StreamHandle {
        StreamHandle {
            play_state: self.play_state.clone(),
            request_new_channel: self.request_new_channel.clone(),
        }
    }

    pub async fn process_packet(&mut self, packet: &ffmpeg_next::Packet) {
        let frames = self.decoder.process_packet(packet);
        for frame in frames {
            self.stream_scheduler.insert_frame(frame).await;
        }
    }
}