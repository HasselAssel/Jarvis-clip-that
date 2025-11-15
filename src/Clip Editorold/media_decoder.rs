use std::collections::HashMap;

use ffmpeg_next::decoder;
use ffmpeg_next::media::Type;
use tokio::sync::mpsc as tokio_mpsc;
use crate::debug_println;

use crate::decoders::{AudioDecoder, DecodedFrame, FfmpegDecoder, VideoDecoder};
use crate::media::Media;
use crate::stream_handle::StreamHandle;

const MAX_BUFFERED_SECS: usize = 2;

pub struct VideoSettings {
    pub width: u32,
    pub height: u32,
}

pub struct AudioSettings;

struct DecoderChannel {
    pub decoder: Box<dyn FfmpegDecoder + Send>,
    sender: tokio_mpsc::Sender<DecodedFrame>,
}

pub struct MediaDecoder {
    pub media: Media,
    pos: u32,
    pub decoders: HashMap<usize, DecoderChannel>,
}

impl MediaDecoder {
    pub fn new(media: Media, video_settings: VideoSettings, audio_settings: AudioSettings) -> (Self, HashMap<usize, StreamHandle<DecodedFrame>>) {
        let mut decoders = HashMap::new();
        let mut stream_handles = HashMap::new();

        for (index, stream) in media.streams.iter() {
            let codec = decoder::find(stream.parameters.id()).unwrap();
            debug_println!("CODEC: {}", codec.name());
            let mut ctx = ffmpeg_next::codec::context::Context::new_with_codec(codec);
            ctx.set_parameters(stream.parameters.clone()).ok().unwrap();
            let params = unsafe { *stream.parameters.as_ptr() };

            let (decoder_channel, rx, rate) = match stream.parameters.medium() {
                Type::Video => {
                    let rate = params.framerate.num as f64 / params.framerate.den as f64;
                    let (tx, rx) = tokio_mpsc::channel(rate as usize * MAX_BUFFERED_SECS);
                    let video_decoder = VideoDecoder {
                        decoder: ctx.decoder().video().ok().unwrap(),
                        out_width: video_settings.width,
                        out_height: video_settings.height,
                    };
                    (DecoderChannel {
                        decoder: Box::new(video_decoder),
                        sender: tx,
                    }, rx, rate)
                }
                Type::Audio => {
                    let rate = params.sample_rate;
                    let (tx, rx) = tokio_mpsc::channel(rate as usize * MAX_BUFFERED_SECS);
                    let audio_decoder = AudioDecoder {
                        decoder: ctx.decoder().audio().ok().unwrap(),
                    };
                    (DecoderChannel {
                        decoder: Box::new(audio_decoder),
                        sender: tx,
                    }, rx, rate as f64)
                }
                _ => todo!()
            };

            decoders.insert(*index, decoder_channel);

            let stream_scheduler = StreamHandle::new(*index, rx, rate, stream.parameters.medium());
            stream_handles.insert(*index, stream_scheduler);
        }

        (Self {
            media,
            pos: 0,
            decoders,
        }, stream_handles)
    }

    pub async fn play(&mut self) {
        let mut packet_iter = self.media.ictx.packets();
        loop {
            if let Some((stream, packet)) = packet_iter.next() {
                if let Some((decoded_channel)) = self.decoders.get_mut(&stream.index()) {
                    for decoded_frame in decoded_channel.decoder.process_packet(&packet) {
                        decoded_channel.sender.send(decoded_frame).await.unwrap();
                    }
                }
            } else {
                break;
            }
        }
    }
}