use std::collections::HashMap;
use std::sync::Arc;

use crate::decoders::DecodedFrame;
use crate::media::Media;
use crate::media_decoder::{AudioSettings, MediaDecoder, VideoSettings};
use crate::stream_handle::StreamHandle;

pub struct MediaPlayback {
    is_playing: bool,

    pub stream_handles: HashMap<usize, StreamHandle<DecodedFrame>>,
    media_decoder: MediaDecoder,
}

impl MediaPlayback {
    pub fn new(
        media: Media,
        video_settings: VideoSettings,
        audio_settings: AudioSettings,
    ) -> Self {
        let (mut media_decoder, mut stream_handles) = MediaDecoder::new(media, video_settings, audio_settings);
        Self {
            is_playing: true,
            stream_handles,
            media_decoder,
        }
    }

    pub fn add_stream_handle_callback(&mut self, stream_index: usize, callback_fn: Arc<dyn Fn(DecodedFrame) + Send + Sync>) -> Option<Option<()>> {
        self.stream_handles.get_mut(&stream_index).map(|stream_handle| stream_handle.set_callback(callback_fn))
    }

    fn start_stream_handles(&mut self) {
        self.stream_handles.iter_mut().for_each(|(_, stream_handle)| stream_handle.start_scheduler());
    }

    pub async fn start(mut self) {
        self.start_stream_handles();
        self.media_decoder.play().await;
    }
}

pub fn create_audio_output_devices() {

}