use std::collections::{HashMap, VecDeque};
use std::time::Duration;

use crossbeam_channel::{Receiver, Sender};
use eframe;
use eframe::egui;
use ffmpeg_next::frame::Video;

use crate::playback::{PlaybackCommand, PlaybackManager};
use crate::wgpu::VideoTexture;

pub struct App {
    video_textures: HashMap<usize, VideoTexture>,
    video_receiver: Receiver<(Video, usize)>,
    video_buffers: HashMap<usize, VecDeque<Video>>,
    cmd_sender: Sender<PlaybackCommand>,
    render_state: egui_wgpu::RenderState,

    playback_clock: Option<PlaybackClock>,
    playing: bool,
}

impl App {
    pub fn new(
        cc: &eframe::CreationContext<'_>,
    ) -> Self {
        let render_state = cc.wgpu_render_state.as_ref().and_then(|rs| Some(rs.clone())).unwrap();
        let (_, video_receiver) = crossbeam_channel::unbounded();
        let (cmd_sender, _) = crossbeam_channel::unbounded();

        Self {
            video_textures: HashMap::new(),
            video_receiver,
            video_buffers: HashMap::new(),
            cmd_sender,
            render_state,
            playback_clock: None,
            playing: false,
        }
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        ctx.input(|input| {
            if let Some(file) = input.raw.dropped_files.last().and_then(|file| file.path) {
                let (cmd_sender, cmd_receiver) = crossbeam_channel::unbounded();
                let (vid_sender, vid_receiver) = crossbeam_channel::unbounded();
                let (aud_sender, aud_receiver) = crossbeam_channel::unbounded();
                self.video_receiver = vid_receiver;

                let (playback_manager, media_info) = PlaybackManager::new(
                    file.as_ref(),
                    Duration::from_millis(500),
                    cmd_receiver,
                    vid_sender,
                    aud_sender
                )?;

                playback_manager.start();

                self.video_textures = media_info.video_streams.iter().map(|(index, params, tb)| {
                    let vt = VideoTexture::new(&self.render_state, 960, 540);
                    (*index, vt)
                }).collect();
            }
        });

        while let Ok((video, index)) = self.video_receiver.try_recv() {
            let buffer = self.video_buffers.entry(index).or_insert_with(|| VecDeque::new());
            buffer.push_back(video);
        }

        for (index, video_buffer) in self.video_buffers {
            if let Some(video) = video_buffer.front() {

            }
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            if ui.button(if self.playing { "Pause" } else { "Play" }).clicked() {
                self.playing = !self.playing;
            }

            let available_size = ui.available_size();
            for (index, video_texture) in &self.video_textures {
                ui.add(
                    egui::Image::new((
                        video_texture.id,
                        egui::vec2(
                            video_texture.width as f32,
                            video_texture.height as f32,
                        ),
                    )).fit_to_exact_size(available_size)
                );
            }

        });

        ctx.request_repaint();
    }
}