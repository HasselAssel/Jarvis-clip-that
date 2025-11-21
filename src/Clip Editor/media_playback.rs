use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use ffmpeg_next::{codec, decoder, media};
use ffmpeg_next::media::Type;
use ffmpeg_next::sys::AVFormatContext;
use rodio::{OutputStream, Sink};
use crate::audio_playback::{frame_to_interleaved_f32, LiveSource};

use crate::decoders::{AudioDecoder, FfmpegDecoder, VideoDecoder};
use crate::egui::WorkerMessage;
use crate::media::Media;
use crate::stream::{Stream, StreamHandle};
use crate::stream_scheduler::{AsyncFnType, DynRateScheduler, FixedRateScheduler, StreamFrameScheduler};

pub struct VideoSettings {
    pub width: u32,
    pub height: u32,
}

pub struct AudioSettings;


pub struct MediaPlayback {
    media: Media,
    video_streams: HashMap<usize, Stream<VideoDecoder>>,
    audio_streams: HashMap<usize, Stream<AudioDecoder>>,
}

impl MediaPlayback {
    pub fn new(media: Media, video_settings: VideoSettings, audio_settings: AudioSettings, max_buffered_seconds: f64) -> Self {
        let mut video_streams = HashMap::new();
        let mut audio_streams = HashMap::new();

        for (index, stream) in media.streams.iter() {
            let codec = decoder::find(stream.parameters.id()).unwrap();
            let mut ctx = codec::context::Context::new_with_codec(codec);
            ctx.set_parameters(stream.parameters.clone()).ok().unwrap();
            let params = unsafe { *stream.parameters.as_ptr() };

            let max_buffered_duration = Duration::from_secs_f64(max_buffered_seconds);

            match stream.parameters.medium() {
                Type::Video => {
                    let call_back: Arc<AsyncFnType<_>> = Arc::new(|_| Box::pin(async {}));
                    let video_decoder = VideoDecoder {
                        decoder: ctx.decoder().video().ok().unwrap(),
                        out_width: video_settings.width,
                        out_height: video_settings.height,
                    };
                    let scheduler: Box<dyn StreamFrameScheduler<_>> = if params.framerate.num == 0 && params.framerate.den == 1 {
                        Box::new(DynRateScheduler::new(max_buffered_duration, call_back))
                    } else {
                        let rate = params.framerate.num as f64 / params.framerate.den as f64;
                        Box::new(FixedRateScheduler::new(rate, max_buffered_seconds, call_back))
                    };
                    let stream = Stream::new(video_decoder, scheduler);
                    video_streams.insert(*index, stream);
                }
                Type::Audio => {
                    let call_back: Arc<AsyncFnType<_>> = Arc::new(|_| Box::pin(async {}));
                    let rate = params.sample_rate as f64; // TODO!!!!!!!!!
                    let audio_decoder = AudioDecoder {
                        decoder: ctx.decoder().audio().ok().unwrap()
                    };
                    let scheduler = Box::new(FixedRateScheduler::new(rate, max_buffered_seconds, call_back));
                    let stream = Stream::new(audio_decoder, scheduler);
                    audio_streams.insert(*index, stream);
                }
                _ => todo!()
            };
        }

        Self {
            media,
            video_streams,
            audio_streams,
        }
    }

    pub fn dummy_callback_insert(&mut self, ctx: eframe::egui::Context, video_sender: std::sync::mpsc::Sender<WorkerMessage>) {
        if let Some((_, stream)) = self.video_streams.iter_mut().find(|(_, _)| true) {
            let video_back_back: Arc<AsyncFnType<_>> = Arc::new(move |video_frame| {
                let video_sender = video_sender.clone();
                let ctx = ctx.clone();
                Box::pin(async move {
                    let message = WorkerMessage::Frame(video_frame);
                    video_sender.send(message).unwrap();
                    ctx.request_repaint();
                })
            });

            stream.stream_scheduler.set_call_back(video_back_back);
        };

        for (i, stream) in &mut self.audio_streams {
            if let Some(stream_info) = self.media.streams.get(&i) {
                let (tx, rx) = std::sync::mpsc::channel();

                let sample_rate = unsafe { (*stream_info.parameters.as_ptr()).sample_rate };
                let channels = unsafe { (*stream_info.parameters.as_ptr()).ch_layout.nb_channels };
                thread::spawn(move || {
                    let (_stream, stream_handle) = OutputStream::try_default().unwrap();
                    let sink = Sink::try_new(&stream_handle).unwrap();
                    let source = LiveSource {
                        receiver: rx,
                        sample_rate: sample_rate.try_into().unwrap(),
                        channels: 1,//channels.try_into().unwrap(),
                    };
                    sink.append(source);
                    sink.sleep_until_end();
                });

                let audio_call_back: Arc<AsyncFnType<_>> = Arc::new(move |audio_frame| {
                    let tx = tx.clone();
                    Box::pin(async move {
                        let audio_data = frame_to_interleaved_f32(&audio_frame);
                        for d in audio_data {
                            tx.send(d).unwrap();
                        }
                    })
                });

                stream.stream_scheduler.set_call_back(audio_call_back);
            }
        }
    }

    pub fn get_handles(&self) -> HashMap<usize, StreamHandle> {
        let mut stream_handles = HashMap::new();

        self.video_streams.iter().for_each(|(i, stream)| { stream_handles.insert(*i, stream.get_stream_handle()); });
        self.audio_streams.iter().for_each(|(i, stream)| { stream_handles.insert(*i, stream.get_stream_handle()); });

        stream_handles
    }

    pub async fn start(mut self) {
        self.video_streams.iter_mut().for_each(|(i, stream)| stream.start());
        self.audio_streams.iter_mut().for_each(|(i, stream)| stream.start());


        let mut packets_iter = self.media.ictx.packets();

        for ((stream, packet)) in packets_iter {
            let packet = packet.clone();
            let index = stream.index();
            let media_type = stream.parameters().medium();

            match media_type {
                Type::Video => {
                    match self.video_streams.get_mut(&index) {
                        Some(stream) => {
                            stream.process_packet(&packet).await;
                        }
                        None => {
                            unreachable!("NUH UH1")
                        }
                    }
                }
                Type::Audio => {
                    match self.audio_streams.get_mut(&index) {
                        Some(stream) => {
                            stream.process_packet(&packet).await;
                        }
                        None => {
                            unreachable!("NUH UH2")
                        }
                    }
                }
                _ => unreachable!("NUH UH3")
            };
        }
    }
}