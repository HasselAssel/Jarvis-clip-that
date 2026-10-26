use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use anyhow::Result;
use anyhow::bail;
use ffmpeg_next::{decoder, Rational, Rescale};
use ffmpeg_next::codec::context::Context;
use ffmpeg_next::codec::Parameters;
use ffmpeg_next::format::context::Input;
use ffmpeg_next::format::Pixel;
use ffmpeg_next::frame::Audio;
use ffmpeg_next::frame::Video;
use ffmpeg_next::media::Type;
use tokio::select;
use tokio::sync::watch;
use tokio::sync::mpsc;
use crate::debug;

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
    if self.playing {
        self.base_position + self.started_at.elapsed()
    } else {
        self.base_position
    }
}
}

pub enum PlaybackCommand {
    Play,
    Pause,
    Seek(Duration),
    Close,
}

pub struct PlaybackManager {
    cmd_receiver: mpsc::UnboundedReceiver<PlaybackCommand>,

    media_decoder: MediaDecoder,
    pub info: MediaInfo,
}

impl PlaybackManager {
    pub fn new(
        path: &str,
        cmd_receiver: mpsc::UnboundedReceiver<PlaybackCommand>,
    ) -> Result<Self> {
        let (media_decoder, info) = MediaDecoder::from_file(path)?;

        Ok(Self {
            cmd_receiver,
            media_decoder,
            info
        })
    }

    pub async fn start(mut self, textures: Vec<(usize, VideoTexture)>) {
        let playback_state = PlaybackState {
            playing: false,
            base_position: Duration::ZERO,
            started_at: Instant::now(),
            seek_id: 0,
        };

        let (send_playback_state, receive_playback_state) = watch::channel(playback_state);

        let video_senders = textures
            .into_iter()
            .map(|(index, texture)| {
                let (send, rec) = mpsc::channel(2);
                let handler = VideoStreamHandler::new(
                    texture,
                    rec,
                    self.info.video_streams
                        .iter()
                        .find(|(i, _, _)| *i == index)
                        .unwrap().2,
                    receive_playback_state.clone(),
                );
                tokio::spawn(async move {
                    let res = handler.start().await;
                    debug!(res);
                });
                (
                    index,
                    send,
                )
            })
            .collect::<HashMap<_, _>>();

        send_playback_state.send_modify(|playback_state| playback_state.seek_id += 1);

        let mut pending_frame = None;

        loop {
            if pending_frame.is_none() {
                pending_frame = self.media_decoder.next_frame();
            }

            select! {
                cmd = self.cmd_receiver.recv() => {
                    if let Some(cmd) = cmd { //TODO: handle error!
                        match cmd {
                            PlaybackCommand::Seek(duration) => {
                                send_playback_state.send_modify(|playback_state| {
                                    playback_state.seek_id += 1;
                                    playback_state.base_position = duration;
                                    playback_state.started_at = Instant::now();
                                });
                                let _ = self.media_decoder.seek(
                                    duration
                                        .as_micros()
                                        .try_into()
                                        .unwrap_or_else(|err| panic!("{} (u128) doesn't fit in i64: {}", duration.as_micros(), err))
                                );
                                pending_frame = None;
                            },
                            PlaybackCommand::Play => {
                                send_playback_state.send_modify(|playback_state| {
                                    if !playback_state.playing {
                                        playback_state.playing = true;
                                        playback_state.started_at = Instant::now();
                                    }
                                });
                            },
                            PlaybackCommand::Pause => {
                                send_playback_state.send_modify(|playback_state| {
                                    if playback_state.playing {
                                        playback_state.playing = false;
                                        playback_state.base_position += playback_state.started_at.elapsed();
                                    }
                                });
                            },
                            PlaybackCommand::Close => {
                                return
                            },
                        }
                    }
                },
                _ = async {
                    let frame = pending_frame.as_ref().unwrap();
                    match frame.frame {
                        Frame::Video(_) => {
                            if let Some(sender) = video_senders.get(&frame.stream) {
                                if let Ok(permit) = sender.reserve().await {
                                    if let Some(frame) = pending_frame.take() {
                                        if let Frame::Video(video) = frame.frame {
                                            permit.send((video, send_playback_state.borrow().seek_id));
                                        }
                                    }
                                }
                            }
                        },
                        Frame::Audio(_) => {
                            /*if let Some(sender) = audio_senders.get(&frame.stream) {
                                if let Ok(permit) = sender.reserve().await {
                                    if let Some(frame) = pending_frame.take() {
                                        if let Frame::Audio(audio) = frame.frame {
                                            permit.send(audio);
                                        }
                                    }
                                }
                            }*/
                            pending_frame.take(); // TODO!
                        }
                    }
                }, if pending_frame.is_some() => {}
            }
        }
    }
}

struct VideoStreamHandler {
    texture: VideoTexture,
    converter: VideoConverter,

    buffer: VecDeque<(Video, u64)>,
    buffer_max: usize,
    frame_receiver: mpsc::Receiver<(Video, u64)>,

    time_base: Rational,
    playback_state: watch::Receiver<PlaybackState>,

    local_seek_id: u64,
}

impl VideoStreamHandler {
    fn new(
        texture: VideoTexture,
        frame_receiver: mpsc::Receiver<(Video, u64)>,
        time_base: Rational,
        playback_state: watch::Receiver<PlaybackState>,
    ) -> Self {
        let local_seek_id = playback_state.borrow().seek_id;
        Self {
            converter: VideoConverter::new(Pixel::RGBA),
            texture,
            buffer: VecDeque::new(),
            buffer_max: (2 as i64).rescale( // TODO bad idea for buffering!
                (1, 1),
                time_base,
            ) as usize,
            frame_receiver,
            time_base,
            local_seek_id,
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
                
                self.local_seek_id = state.seek_id;

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
                msg = self.frame_receiver.recv(),
                    if self.buffer.len() < self.buffer_max
                => {
                    match msg {
                        Some((video, seek_id)) => {
                            if seek_id == self.playback_state.borrow().seek_id {
                                let converted = self.converter.convert(&video)?;
                                self.buffer.push_back((converted, seek_id));
                            }
                        }
                        None => bail!("frame_receiver done!"),
                    }
                }

                changed = self.playback_state.changed() => {
                    if changed.is_err() {
                        bail!("state broken, idk");
                    }
                }

                _ = tokio::time::sleep(delay.unwrap_or(Duration::MAX)),
                    if delay.is_some() => {}
            }
        }
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

    pub duration: Duration,
}

struct MediaDecoder {
    input: Input,

    video_decoders: HashMap<usize, decoder::Video>,
    audio_decoders: HashMap<usize, decoder::Audio>,

    pending: VecDeque<DecodedFrame>,
}

impl MediaDecoder {
    pub fn from_file(path: &str) -> Result<(Self, MediaInfo)> {
        let mut input = ffmpeg_next::format::input(path)?;

        let duration = Duration::from_micros(input.duration() as u64);

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
                duration,
            }
        ))
    }

    fn next_frame(&mut self) -> Option<DecodedFrame> {
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
                        self.pending.push_back(DecodedFrame {
                            frame: Frame::Video(frame),
                            stream: stream.index(),
                        });
                        frame = Video::empty();
                    }
                }
                Type::Audio => {
                    let decoder = self.audio_decoders.get_mut(&index)?;
                    decoder.send_packet(&packet).ok()?;

                    let mut frame = Audio::empty();
                    while decoder.receive_frame(&mut frame).is_ok() {
                        self.pending.push_back(DecodedFrame {
                            frame: Frame::Audio(frame),
                            stream: stream.index(),
                        });
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