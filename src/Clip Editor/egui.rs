use std::collections::{HashMap, VecDeque};
use std::time::Duration;

use eframe;
use eframe::egui;
use tokio::sync::mpsc;

use crate::playback::{PlaybackCommand, PlaybackManager};
use crate::wgpu::VideoTexture;
use crate::wgpu::VideoTextureHandle;
use crate::debug;

pub struct GUI {
    render_state: egui_wgpu::RenderState,
    
    video_textures: Vec<(usize, VideoTextureHandle)>,
    cmd_sender: Option<mpsc::UnboundedSender<PlaybackCommand>>,

    playing: bool,
    seek_seconds: f64,
    duration_seconds: f64,
}

impl GUI {
    pub fn new(
        cc: &eframe::CreationContext<'_>,
    ) -> Self {
        let render_state = cc.wgpu_render_state.as_ref().and_then(|rs| Some(rs.clone())).unwrap();

        Self {
            video_textures: Vec::new(),
            cmd_sender: None,
            render_state,
            playing: false,
            seek_seconds: 0.,
            duration_seconds: 0.,
        }
    }
}

impl eframe::App for GUI {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        ctx.input(|input| {
            if let Some(path) = input.raw.dropped_files.last()
                                        .and_then(|file| file.path.as_ref())
                                        .and_then(|path| path.to_str()) {
                self.open_file(ctx, path);
            }
        });

        egui::TopBottomPanel::bottom("timeline")
            .resizable(true)
            .default_height(150.0)
            .min_height(80.0)
            .show(ctx, |ui| {
                ui.heading("Timeline");

                ui.separator();

                ui.label("Video tracks and clip information will go here.");
            });

        egui::TopBottomPanel::bottom("controls")
            .resizable(false)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    if ui.button(if self.playing { "⏸" } else { "▶" }).clicked() {
                        self.playing = !self.playing;

                        if let Some(tx) = &self.cmd_sender {
                            let _ = tx.send(if self.playing {
                                PlaybackCommand::Play
                            } else {
                                PlaybackCommand::Pause
                            });
                        }
                    }

                    fn format_time(seconds: f64) -> String {
                        let total = seconds as u64;

                        format!(
                            "{:02}:{:02}:{:02}",
                            total / 3600,
                            (total / 60) % 60,
                            total % 60,
                        )
                    }

                    let time_text = format!(
                        "{} / {}",
                        format_time(self.seek_seconds),
                        format_time(self.duration_seconds),
                    );

                    let label_width = ui.fonts(|fonts| {
                        fonts.layout_no_wrap(
                            time_text.clone(),
                            egui::TextStyle::Body.resolve(ui.style()),
                            ui.visuals().text_color(),
                        ).size().x
                    });

                    let slider_width = (
                        ui.available_width()
                        - label_width
                        - ui.spacing().item_spacing.x
                    ).max(0.0);

                    ui.spacing_mut().slider_width = slider_width;

                    let response = ui.add(
                        egui::Slider::new(
                            &mut self.seek_seconds,
                            0.0..=self.duration_seconds.max(0.0),
                        )
                        .smart_aim(false)
                        .step_by(0.01)
                        .show_value(false),
                    );

                    ui.label(time_text);

                    if response.drag_stopped() || (response.changed() && !response.dragged()){
                        if let Some(tx) = &self.cmd_sender {
                            let _ = tx.send(PlaybackCommand::Seek(
                                Duration::from_secs_f64(self.seek_seconds),
                            ));
                        }
                    }
                });
            });

        egui::CentralPanel::default().show(ctx, |ui| {
            let available = ui.available_size();

            for (_, texture_handle) in &self.video_textures {
                ui.add(
                    egui::Image::new((
                        texture_handle.id,
                        ui.available_size(),
                    )).maintain_aspect_ratio(true)
                );
            }
        });
    }
}

impl GUI {
    fn open_file(&mut self, ctx: &egui::Context, path: &str) {
        debug!(path);
        let (tx, rx) = mpsc::unbounded_channel();
        if let Ok(playback_manager) = PlaybackManager::new(
            path,
            rx,
        ) {
            if let Some(send) = &self.cmd_sender {
                let _ = send.send(PlaybackCommand::Close);
            }
            self.cmd_sender = Some(tx);
            self.playing = false;

            self.seek_seconds = 0.;
            self.duration_seconds = playback_manager.info.duration.as_secs_f64();

            let textures = playback_manager.info.video_streams
                .iter()
                .map(|(i, _, _)|
                    (*i,
                    VideoTexture::new(
                        &self.render_state,
                        self.render_state.queue.clone(),
                        ctx.clone(),
                    ))
                )
                .collect::<Vec<_>>();

            self.video_textures = textures
                .iter()
                .map(|(i, vt)| (*i, vt.handle()))
                .collect();

            tokio::spawn(async move {
                playback_manager.start(textures).await;
            });
        }
    }
}