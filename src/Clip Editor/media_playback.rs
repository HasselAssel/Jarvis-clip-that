use std::collections::HashMap;
use crate::decoders::{AudioDecoder, FfmpegDecoder, VideoDecoder};
use crate::stream::Stream;
use crate::stream_scheduler::StreamFrameScheduler;

struct MediaPlayback {
    video_streams: HashMap<u32, Stream<VideoDecoder, dyn StreamFrameScheduler<<VideoDecoder as FfmpegDecoder>::DecodedFrame>>>,
    audio_streams: HashMap<u32, Stream<AudioDecoder, dyn StreamFrameScheduler<<AudioDecoder as FfmpegDecoder>::DecodedFrame>>>,
}