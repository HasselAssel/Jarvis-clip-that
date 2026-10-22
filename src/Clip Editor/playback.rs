use std::collections::{HashMap, VecDeque};
use std::time::Duration;

use anyhow::Result;
use crossbeam_channel::Receiver;
use ffmpeg_next::codec::context::Context;
use ffmpeg_next::decoder;
use ffmpeg_next::format::context::Input;
use ffmpeg_next::frame::Audio;
use ffmpeg_next::frame::Video;
use ffmpeg_next::media::Type;

pub enum PlaybackCommand {
    Play,
    Pause,
    Seek(Duration)
}

pub struct PlaybackManager {
    playing: bool,
    position: Duration,
    cmd_receiver: Receiver<PlaybackCommand>,

    media_decoder: MediaDecoder,
}

impl PlaybackManager {
    pub fn new(
        path: &str,
        cmd_receiver: Receiver<PlaybackCommand>,
    ) -> Result<Self> {
        let media_decoder = MediaDecoder::from_file(path)?;
        Ok(Self {
            playing: false,
            position: Duration::ZERO,
            cmd_receiver,
            media_decoder
        })
    }
}

enum DecodedFrame {
    Video(Video),
    Audio(Audio),
}

struct MediaDecoder {
    input: Input,

    video_decoders: HashMap<usize, decoder::Video>,
    audio_decoders: HashMap<usize, decoder::Audio>,

    pending: VecDeque<DecodedFrame>,
}

impl MediaDecoder {
    pub fn from_file(path: &str) -> Result<Self> {
        let mut input = ffmpeg_next::format::input(path)?;

        let mut video_decoders = HashMap::new();
        let mut audio_decoders = HashMap::new();

        for stream in input.streams() {
            let context = Context::from_parameters(
                stream.parameters()
            )?;
            let index = stream.index();
            match stream.parameters().medium() {
                Type::Video => {
                    let decoder = context.decoder().video()?;
                    video_decoders.insert(index, decoder);
                },
                Type::Audio => {
                    let decoder = context.decoder().audio()?;
                    audio_decoders.insert(index, decoder);
                },
                _ => {}
            }
        }

        Ok(Self {
            input,
            video_decoders,
            audio_decoders,
            pending: VecDeque::new(),
        })
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
                    decoder.send_packet(&packet).ok();

                    let mut frame = Video::empty();
                    while decoder.receive_frame(&mut frame).is_ok() {
                        self.pending.push_back(DecodedFrame::Video(frame));
                        frame = Video::empty();
                    }
                }
                Type::Audio => {
                    let decoder = self.audio_decoders.get_mut(&index)?;
                    decoder.send_packet(&packet).ok();

                    let mut frame = Audio::empty();
                    while decoder.receive_frame(&mut frame).is_ok() {
                        self.pending.push_back(DecodedFrame::Audio(frame));
                        frame = Audio::empty();
                    }
                }
                _ => {}
            }
        }
    }

    fn seek(&mut self, microseconds: i64) -> Result<()> {
        self.input.seek(microseconds, ..microseconds)?;
        Ok(())
    }
}