use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::thread;
use std::time::Duration;
use atomic_float::AtomicF32;

use ffmpeg_next::{codec, decoder};
use ffmpeg_next::media::Type;
use rodio::{OutputStream, Sink};
use rodio::dynamic_mixer::{DynamicMixer, DynamicMixerController};
use rodio::source::UniformSourceIterator;

use crate::audio_playback::{frame_to_interleaved_f32, LiveSource};
use crate::decoders::{AudioDecoder, VideoDecoder};
use crate::egui::WorkerMessage;
use crate::media::{Media, StreamInfo};
use crate::stream::{Stream, StreamHandle};
use crate::stream_scheduler::{AsyncFnType, DynRateScheduler, FixedRateScheduler, StreamFrameScheduler};

pub struct VideoSettings {
    pub width: u32,
    pub height: u32,
}

pub struct AudioSettings {
    pub initial_vol: f32,
}


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

            let request_new_channel = Arc::new(AtomicBool::new(false));

            match stream.parameters.medium() {
                Type::Video => {
                    let call_back: Arc<AsyncFnType<_>> = Arc::new(|_| Box::pin(async {}));
                    let video_decoder = VideoDecoder {
                        decoder: ctx.decoder().video().ok().unwrap(),
                        out_width: video_settings.width,
                        out_height: video_settings.height,
                    };
                    let scheduler: Box<dyn StreamFrameScheduler<_>> = if params.framerate.num == 0 && params.framerate.den == 1 {
                        Box::new(DynRateScheduler::new(max_buffered_duration, call_back, request_new_channel))
                    } else {
                        let rate = params.framerate.num as f64 / params.framerate.den as f64;
                        Box::new(FixedRateScheduler::new(rate, max_buffered_seconds, call_back, request_new_channel))
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
                    let scheduler = Box::new(FixedRateScheduler::new(rate, max_buffered_seconds, call_back, request_new_channel));
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

    pub async fn dummy_callback_insert(&mut self, ctx: eframe::egui::Context, video_sender: &std::sync::mpsc::Sender<WorkerMessage>) -> HashMap<usize, Arc<AtomicF32>> {
        if let Some((i, stream)) = self.video_streams.iter_mut().find(|(_, _)| true) {
            let time_base = self.media.streams.get(i).and_then(|stream_info| unsafe {
                let rational = (*stream_info.parameters.as_ptr()).framerate;
                (rational.num != 0 || rational.den != 1).then(|| rational.den as f32 / rational.num as f32)
            });

            let video_sender = video_sender.clone();
            let video_back_back: Arc<AsyncFnType<_>> = Arc::new(move |video_frame| {
                let video_sender = video_sender.clone();
                let ctx = ctx.clone();
                Box::pin(async move {
                    let message = WorkerMessage::Frame(video_frame, time_base);
                    video_sender.send(message).unwrap();
                    ctx.request_repaint();
                })
            });

            stream.stream_scheduler.set_call_back(video_back_back);
        };

        let mut volumes = HashMap::new();
        for (i, stream) in &mut self.audio_streams {
            if let Some(stream_info) = self.media.streams.get(&i) {
                let (tx, rx) = std::sync::mpsc::channel();

                {
                    let sample_rate = unsafe { (*stream_info.parameters.as_ptr()).sample_rate };
                    let channels = 1; //FOR SOME REASON IDK???!?!?!? MAYBE CAUSE THE DATA IS PLANAR??????ANYYWAY; MAYBE THE FFMPEG GODS????????????????????????????????????????????????????????????????????????????????????????//let channels = unsafe { (*stream_info.parameters.as_ptr()).ch_layout.nb_channels };

                    let volume = Arc::new(AtomicF32::new(0.5));
                    let volume_ = volume.clone();
                    volumes.insert(*i, volume);
                    let source = LiveSource {
                        receiver: rx,
                        sample_rate: sample_rate as u32,
                        channels,
                        volume: volume_,
                    };
                    thread::spawn(|| {
                        let (_stream, stream_handle) = OutputStream::try_default().unwrap();
                        let sink = Sink::try_new(&stream_handle).unwrap();
                        sink.append(source);
                        sink.sleep_until_end();
                    });
                }


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
        volumes
    }

    pub fn get_handles(&self) -> HashMap<usize, StreamHandle> {
        let mut stream_handles = HashMap::new();

        self.video_streams.iter().for_each(|(i, stream)| { stream_handles.insert(*i, stream.get_stream_handle()); });
        self.audio_streams.iter().for_each(|(i, stream)| { stream_handles.insert(*i, stream.get_stream_handle()); });

        stream_handles
    }

    pub async fn start(mut self, (seeking_seconds_f32, cond): (Arc<AtomicU32>, Arc<AtomicBool>)) {
        self.video_streams.iter_mut().for_each(|(_, stream)| { stream.start(); });
        self.audio_streams.iter_mut().for_each(|(_, stream)| { stream.start(); });

        loop {
            let secs_ = seeking_seconds_f32.load(Ordering::SeqCst);
            let secs = f32::from_bits(secs_);
            let index = secs as f64 * ffmpeg_next::sys::AV_TIME_BASE as f64;
            self.media.ictx.seek(index as i64, std::ops::RangeFull).unwrap();
            let packets_iter = self.media.ictx.packets();

            for (stream, packet) in packets_iter {
                if cond.load(Ordering::SeqCst) {
                    cond.store(false, Ordering::SeqCst);
                    break;
                }
                let index = stream.index();
                let media_type = stream.parameters().medium();

                match media_type {
                    Type::Video => {
                        eprintln!("Video!");
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
                        eprintln!("Audio!");
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
}