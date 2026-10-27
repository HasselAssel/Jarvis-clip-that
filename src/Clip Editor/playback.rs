use std::collections::{HashMap, VecDeque};
use std::time::{Duration, Instant};
use rodio::{DeviceSinkBuilder, Player, Source};

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
use std::num::NonZero;
use tokio::select;
use tokio::sync::watch;
use tokio::sync::mpsc;
use crate::debug;

use crate::frame_converter::VideoConverter;
use crate::frame_converter::AudioConverter;
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

    send_playback_state: watch::Sender<PlaybackState>,
    pub receive_playback_state: watch::Receiver<PlaybackState>,

    media_decoder: MediaDecoder,
    pub info: MediaInfo,
}

impl PlaybackManager {
    pub fn new(
        path: &str,
        cmd_receiver: mpsc::UnboundedReceiver<PlaybackCommand>,
    ) -> Result<Self> {
        let (media_decoder, info) = MediaDecoder::from_file(path)?;

        let playback_state = PlaybackState {
            playing: false,
            base_position: Duration::ZERO,
            started_at: Instant::now(),
            seek_id: 0,
        };
        let (send_playback_state, receive_playback_state) = watch::channel(playback_state);

        Ok(Self {
            cmd_receiver,
            send_playback_state,
            receive_playback_state,
            media_decoder,
            info,
        })
    }

    pub async fn start(mut self, textures: Vec<(usize, VideoTexture)>) {
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
                    self.receive_playback_state.clone(),
                );
                tokio::spawn(async move {
                    let res = handler.start().await;
                    let _ = debug!(res);
                });
                (
                    index,
                    send,
                )
            })
            .collect::<HashMap<_, _>>();

        let output = DeviceSinkBuilder::open_default_sink().unwrap();

        let audio_senders = self.info.audio_streams
            .iter()
            .map(|(index, _param, tb, rate)| {
                let (send, rec) = mpsc::channel(2);
                let handler = AudioStreamHandler::new(
                    rec,
                    self.receive_playback_state.clone(),
                    tb.clone(),
                );
                let rate = *rate;
                let player = Player::connect_new(output.mixer());
                tokio::spawn(async move {
                    let res = handler.start(player, rate).await;
                    let _ = debug!(res);
                });
                (
                    index,
                    send,
                )
            })
            .collect::<HashMap<_, _>>();

        self.send_playback_state.send_modify(|playback_state| playback_state.seek_id += 1);

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
                                self.send_playback_state.send_modify(|playback_state| {
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
                                self.send_playback_state.send_modify(|playback_state| {
                                    if !playback_state.playing {
                                        playback_state.playing = true;
                                        playback_state.started_at = Instant::now();
                                    }
                                });
                            },
                            PlaybackCommand::Pause => {
                                self.send_playback_state.send_modify(|playback_state| {
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
                                            permit.send((video, self.send_playback_state.borrow().seek_id));
                                        }
                                    }
                                }
                            }
                        },
                        Frame::Audio(_) => {
                            if let Some(sender) = audio_senders.get(&frame.stream) {
                                if let Ok(permit) = sender.reserve().await {
                                    if let Some(frame) = pending_frame.take() {
                                        if let Frame::Audio(audio) = frame.frame {
                                            permit.send((audio, self.send_playback_state.borrow().seek_id));
                                        }
                                    }
                                }
                            }
                        }
                    }
                }, if pending_frame.is_some() => {}
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

#[derive(Clone)]
pub struct MediaInfo {
    pub video_streams: Vec<(usize, Parameters, Rational)>,
    pub audio_streams: Vec<(usize, Parameters, Rational, u32)>, // index, params, tb, rate
    pub stream_metadata: HashMap<usize, HashMap<String, String>>,

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
        let input = ffmpeg_next::format::input(path)?;

        let duration = Duration::from_micros(input.duration() as u64);

        let mut video_streams = Vec::new();
        let mut audio_streams = Vec::new();

        let mut stream_metadata = HashMap::new();
        let mut video_decoders = HashMap::new();
        let mut audio_decoders = HashMap::new();

        for stream in input.streams() {
            let metadata = stream.metadata()
                .iter()
                .map(|(key, value)| (key.to_string(), value.to_string()))
                .collect();

            stream_metadata.insert(stream.index(), metadata);

            let context = Context::from_parameters(
                stream.parameters()
            )?;
            let index = stream.index();
            match stream.parameters().medium() {
                Type::Video => {
                    let decoder = context.decoder().video()?;
                    video_streams.push((index, stream.parameters(), stream.time_base()));
                    video_decoders.insert(index, decoder);
                },
                Type::Audio => {
                    let decoder = context.decoder().audio()?;
                    audio_streams.push((index, stream.parameters(), stream.time_base(), decoder.rate()));
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
                stream_metadata,
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
                            if seek_id >= self.playback_state.borrow().seek_id {
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

pub struct AudioStreamHandler {
    receiver: mpsc::Receiver<(Audio, u64)>,
    playback_state: watch::Receiver<PlaybackState>,
    converter: AudioConverter,
    time_base: Rational,
}

impl AudioStreamHandler {
    pub fn new(
        receiver: mpsc::Receiver<(Audio, u64)>,
        playback_state: watch::Receiver<PlaybackState>,
        time_base: Rational,
    ) -> Self {
        let converter = AudioConverter::new();
        Self {
            receiver,
            playback_state,
            converter,
            time_base,
        }
    }

    pub async fn start(mut self, player: Player, sample_rate: u32) -> Result<()> {
        let (mut sender, receiver) = mpsc::channel(100);

        let source = AudioSource::new(
            receiver,
            1000,
            sample_rate,
        );

        player.pause();
        player.append(source);

        let mut player_seek_id = self.playback_state.borrow().seek_id;
        let mut playing = self.playback_state.borrow().playing;
        if playing {
            player.play();
        }

        let mut pending = None;
        let mut needs_alignment = true;

        loop {
            select! {
                permit = sender.clone().reserve_owned(), if pending.is_some() => {
                    let permit = permit?;
                    permit.send(pending.take().unwrap());
                },

                msg = self.receiver.recv(), if pending.is_none()
                => {
                    match msg {
                        Some((audio, seek_id)) => {
                            if seek_id != player_seek_id {
                                continue;
                            }
                            let converted = self.converter.resample(&audio)?;
                            let samples: &[f32] = converted.plane(0);
                            let mut buffer = VecDeque::from(samples.to_vec());

                            if needs_alignment {
                                if let Some(pts) = audio.pts() {
                                    let frame_start = pts.rescale(
                                        self.time_base,
                                        Rational(1, sample_rate as i32),
                                    );

                                    let target = self.playback_state.borrow()
                                        .position()
                                        .as_micros() as i64
                                        * sample_rate as i64 / 1_000_000;

                                    let delta = frame_start - target;
                                    let channels = 2;

                                    if delta < 0 { // skip samples
                                        let skip = (-delta) as usize * channels;
                                        buffer.drain(..skip.min(buffer.len()));
                                    } else if delta > 0 { // prepend silence
                                        let silence = delta as usize * channels;
                                        buffer.resize(buffer.len() + silence, 0.0);
                                        buffer.rotate_right(silence);
                                    }

                                    needs_alignment = buffer.is_empty();

                                    if !needs_alignment && self.playback_state.borrow().playing {
                                        player.play();
                                    }
                                }
                            }

                            if !buffer.is_empty() {
                                if let Err(err) = sender.try_send(buffer) {
                                    pending = Some(err.into_inner());
                                }
                            }
                        }
                        None => bail!("frame_receiver done!"),
                    }
                },
                
                changed = self.playback_state.changed() => {
                    if changed.is_err() {
                        bail!("state broken, idk");
                    }
                    let new_playing = self.playback_state.borrow().playing;
                    let new_seek_id = self.playback_state.borrow().seek_id;
                    if new_seek_id != player_seek_id {
                        player.clear();
                        needs_alignment = true;
                        let (new_sender, new_receiver) = mpsc::channel(100);
                        sender = new_sender;
                        let source = AudioSource::new(
                            new_receiver,
                            1000,
                            sample_rate,
                        );

                        player.pause();
                        player.append(source);

                        pending = None;

                        player_seek_id = self.playback_state.borrow().seek_id;
                        playing = self.playback_state.borrow().playing;
                        if playing {
                            player.play();
                        }
                    }
                    if playing != new_playing {
                        playing = new_playing;
                        if playing {
                            player.play();
                        } else {
                            player.pause();
                        }
                    }
                }
            }
        }
    }
}

pub struct AudioSource {
    receiver: mpsc::Receiver<VecDeque<f32>>,
    buffer: VecDeque<VecDeque<f32>>,
    buffer_max: usize,

    sample_rate: u32,
}

impl AudioSource {
    pub fn new(
        receiver: mpsc::Receiver<VecDeque<f32>>,
        buffer_max: usize,
        sample_rate: u32,
    ) -> Self {
        Self {
            receiver,
            buffer: VecDeque::new(),
            buffer_max,
            sample_rate,
        }
    }
}

impl Iterator for AudioSource {
    type Item = f32;

    fn next(&mut self) -> Option<Self::Item> {
        while self.buffer.len() < self.buffer_max {
            if let Ok(sample) = self.receiver.try_recv() {
                if !sample.is_empty() {
                    self.buffer.push_back(sample);
                }
            } else {
                break
            }
        }

        let next = self.buffer.front_mut().and_then(|inner| inner.pop_front());
            
        if self.buffer.front().is_some_and(|inner| inner.is_empty()) {
            self.buffer.pop_front();
        }

        return Some(
            next.unwrap_or(0.0)
        )
    }
}

impl Source for AudioSource {
    fn current_span_len(&self) -> Option<usize> {
        None
    }

    fn channels(&self) -> NonZero<u16> {
        NonZero::new(2).unwrap()
    }

    fn sample_rate(&self) -> NonZero<u32> {
        NonZero::new(self.sample_rate).unwrap()
    }

    fn total_duration(&self) -> Option<Duration> {
        None
    }
}