use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use anyhow::Result;
use crossbeam_channel::Receiver;
use ffmpeg_next::{decoder, Rational, Rescale};
use ffmpeg_next::codec::context::Context;
use ffmpeg_next::codec::Parameters;
use ffmpeg_next::format::context::Input;
use ffmpeg_next::format::Pixel;
use ffmpeg_next::frame::Audio;
use ffmpeg_next::frame::Video;
use ffmpeg_next::media::Type;
use tokio::select;
use tokio::sync::{RwLock, watch};
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver};

use crate::frame_converter::VideoConverter;
use crate::wgpu::VideoTexture;

#[derive(Clone, Debug)]
pub struct PlaybackState {
    pub playing: bool,
    pub base_position: Duration,
    pub started_at: Instant,
    pub seek_id: u64,
}

impl PlaybackState {
    pub fn position(&self) -> Duration {
        self.base_position.saturating_add(self.started_at.elapsed())
    }
}

pub enum PlaybackCommand {
    Consumed(Duration),
    Seek(Duration),
    Close,
}

pub struct PlaybackManager {
    cmd_receiver: Receiver<PlaybackCommand>,

    buffering: Duration,

    media_decoder: MediaDecoder,
    info: MediaInfo,
}

impl PlaybackManager {
    pub fn new(
        path: &str,
        buffering: Duration,
        cmd_receiver: Receiver<PlaybackCommand>,
    ) -> Result<Self> {
        let (media_decoder, info) = MediaDecoder::from_file(path)?;

        Ok(Self {
            cmd_receiver,
            buffering,
            media_decoder,
            info
        })
    }

    pub async fn start(mut self, textures: Vec<(usize, VideoTexture)>) {
        let _playback_state = PlaybackState {
            playing: false,
            base_position: Duration::ZERO,
            started_at: Instant::now(),
            seek_id: 0,
        };

        let playback_state = RwLock::new(_playback_state);

        self.send_buffering_frames(self.last_decoded_pos);

        while let Ok(cmd) = self.cmd_receiver.recv() {
            match cmd {
                PlaybackCommand::Consumed(duration) => {
                    self.send_buffering_frames(duration);
                },
                PlaybackCommand::Seek(duration) => {
                    let _ = self.media_decoder.seek(
                        duration
                            .as_micros()
                            .try_into()
                            .unwrap_or_else(|err| panic!("{} (u128) doesn't fit in i64: {}", duration.as_micros(), err))
                    );
                    self.last_decoded_pos = duration;
                    self.send_buffering_frames(self.last_decoded_pos);
                },
                PlaybackCommand::Close => {
                    return
                },
            }
        }
    }

    fn send_buffering_frames(&mut self, start_pos: Duration) -> Option<()> {
        while self.last_decoded_pos < start_pos + self.buffering {
            let (frame, tb) = self.media_decoder.next_frame()?;
            let Some(pts) = (match frame.frame {
                Frame::Video(video) => {
                    let pts = video.pts();
                    self.video_frame_sender.send((video, frame.stream)).ok()?;
                    pts
                },
                Frame::Audio(audio) => {
                    let pts = audio.pts();
                    self.audio_frame_sender.send((audio, frame.stream)).ok()?;
                    pts
                },
            }) else {
                continue
            };
            let nanos = pts.rescale(tb, (1, 1_000_000_000));
            let duration = Duration::from_nanos(
                nanos
                    .try_into()
                    .unwrap_or_else(|err| panic!("{} (i64) doesn't fit in u64: {}", nanos, err))
            );
            self.last_decoded_pos = duration;
        }

        Some(())
    }
}

struct VideoStreamHandler {
    texture: VideoTexture,
    converter: VideoConverter,

    buffer: VecDeque<(Video, u64)>,
    frame_receiver: UnboundedReceiver<(Video, u64)>,

    time_base: Rational,
    playback_state: watch::Receiver<PlaybackState>,

    local_seek_id: u64,
}

impl VideoStreamHandler {
    fn new(
        texture: VideoTexture,
        frame_receiver: UnboundedReceiver<(Video, u64)>,
        time_base: Rational,
        playback_state: watch::Receiver<PlaybackState>,
    ) -> Self {
        Self {
            converter: VideoConverter::new(
                (texture.width, texture.height),
                Pixel::RGBA,
            ),
            texture,
            buffer: VecDeque::new(),
            frame_receiver,
            time_base,
            local_seek_id: playback_state.borrow().seek_id,
            playback_state,
        }
    }

    fn frame_position(&self, video: &Video) -> Option<Duration> {
        let pts = video.pts()?;
        let nanos = pts.rescale(
            self.time_base,
            (1, 1_000_000_000),
        );

        Some(Duration::from_nanos(nanos.max(0) as u64))
    }

    fn display(&mut self, video: Video) -> Result<()> {
        self.texture.update(&video)?;
        Ok(())
    }

    fn present_frames(&mut self, state: &PlaybackState) -> Result<()> {
        let position = state.position();

        if self.local_seek_id != state.seek_id {
            self.buffer.retain(|(_, id)| *id >= state.seek_id);
            self.local_seek_id = state.seek_id;

            while let Some((video, _)) = self.buffer.front() {
                let Some(pts) = self.frame_position(video) else {
                    self.buffer.pop_front();
                    continue;
                };

                if pts < position {
                    self.buffer.pop_front();
                    continue;
                }

                let (video, _) = self.buffer.pop_front().unwrap();
                self.display(video)?;

                break;
            }

            return Ok(())
        }

        if !state.playing {
            return Ok(());
        }

        let mut latest = None;

        while let Some((video, _)) = self.buffer.front() {
            let Some(pts) = self.frame_position(video) else {
                self.buffer.pop_front();
                continue;
            };

            if pts > position {
                break;
            }

            latest = self.buffer.pop_front().map(|(video, _)| video);
        }

        if let Some(video) = latest {
            self.display(video)?;
        }

        Ok(())
    }

    fn next_delay(&self, state: &PlaybackState) -> Option<Duration> {
        if !state.playing {
            return None;
        }

        let (video, _) = self.buffer.front()?;

        Some(
            self.frame_position(video)?.saturating_sub(state.position())
        )
    }

    async fn start(mut self) -> Result<()> {
        loop {
            let state = self.playback_state.borrow().clone();

            self.present_frames(&state)?;

            let delay = self.next_delay(&state);

            select! {
                msg = self.frame_receiver.recv() => {
                    match msg {
                        Some((video, seek_id)) => {
                            if seek_id == self.local_seek_id {
                                let converted = self.converter.convert(&video)?;
                                self.buffer.push_back((converted, seek_id));
                            }
                        }
                        None => break,
                    }
                }

                changed = self.playback_state.changed() => {
                    if changed.is_err() {
                        break;
                    }
                }

                _ = async {
                    match delay {
                        Some(delay) => {
                            tokio::time::sleep(delay).await;
                        }
                        None => {
                            std::future::pending::<()>().await;
                        }
                    }
                } => {}
            }
        }

        Ok(())
    }
}


struct DecodedFrame {
    frame: Frame,
    stream: usize,
}

enum Frame {
    Video(Video),
    Audio(Audio),
}

pub struct MediaInfo {
    pub video_streams: Vec<(usize, Parameters, Rational)>,
    pub audio_streams: Vec<(usize, Parameters, Rational)>,
}

struct MediaDecoder {
    input: Input,

    video_decoders: HashMap<usize, decoder::Video>,
    audio_decoders: HashMap<usize, decoder::Audio>,

    pending: VecDeque<(DecodedFrame, Rational)>,
}

impl MediaDecoder {
    pub fn from_file(path: &str) -> Result<(Self, MediaInfo)> {
        let mut input = ffmpeg_next::format::input(path)?;

        let mut video_streams = Vec::new();
        let mut audio_streams = Vec::new();

        let mut video_decoders = HashMap::new();
        let mut audio_decoders = HashMap::new();

        for stream in input.streams() {
            let context = Context::from_parameters(
                stream.parameters()
            )?;
            let index = stream.index();
            match stream.parameters().medium() {
                Type::Video => {
                    video_streams.push((index, stream.parameters(), stream.time_base()));
                    let decoder = context.decoder().video()?;
                    video_decoders.insert(index, decoder);
                },
                Type::Audio => {
                    audio_streams.push((index, stream.parameters(), stream.time_base()));
                    let decoder = context.decoder().audio()?;
                    audio_decoders.insert(index, decoder);
                },
                _ => {}
            }
        }

        Ok((
            Self {
                input,
                video_decoders,
                audio_decoders,
                pending: VecDeque::new(),
            },
            MediaInfo {
                video_streams,
                audio_streams,
            }
        ))
    }

    fn next_frame(&mut self) -> Option<(DecodedFrame, Rational)> {
        loop {
            if let Some(frame) = self.pending.pop_front() {
                return Some(frame);
            }

            let (stream, packet) = self.input.packets().next()?;
            let index = stream.index();

            match stream.parameters().medium() {
                Type::Video => {
                    let decoder = self.video_decoders.get_mut(&index)?;
                    decoder.send_packet(&packet).ok()?;

                    let mut frame = Video::empty();
                    while decoder.receive_frame(&mut frame).is_ok() {
                        self.pending.push_back((DecodedFrame {
                            frame: Frame::Video(frame),
                            stream: stream.index(),
                        }, stream.time_base()));
                        frame = Video::empty();
                    }
                }
                Type::Audio => {
                    let decoder = self.audio_decoders.get_mut(&index)?;
                    decoder.send_packet(&packet).ok()?;

                    let mut frame = Audio::empty();
                    while decoder.receive_frame(&mut frame).is_ok() {
                        self.pending.push_back((DecodedFrame {
                            frame: Frame::Audio(frame),
                            stream: stream.index(),
                        }, stream.time_base()));
                        frame = Audio::empty();
                    }
                }
                _ => {}
            }
        }
    }

    fn seek(&mut self, microseconds: i64) -> Result<()> {
        self.input.seek(microseconds, ..microseconds)?;
        self.pending.clear();

        for decoder in self.video_decoders.values_mut() {
            decoder.flush();
        }

        for decoder in self.audio_decoders.values_mut() {
            decoder.flush();
        }

        Ok(())
    }
}