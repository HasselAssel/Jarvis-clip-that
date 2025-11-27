use std::collections::HashMap;
use std::ops::RangeInclusive;
use std::sync::mpsc as sync_mpsc;
use atomic_float::AtomicF32;

use eframe::{CreationContext, egui, wgpu};
use eframe::epaint::TextureId;
use eframe::wgpu::FilterMode;
use ffmpeg_next::frame;
use tokio::sync::mpsc as tokio_mpsc;
use tokio::sync::oneshot;

use crate::textures;

pub enum GUIMessage {
    VideoStateChange(Option<bool>),
    VideoPosChanged(f32),
    VolumeChanged(f32, usize),
}

pub enum WorkerMessage {
    Frame(frame::Video, Option<f32>),
    AddAudioTrack(usize),
}


struct AudioUI {
    volume: f32,
    volume_range: RangeInclusive<f32>,
}

struct VideoUI {
    texture_id: TextureId,
    texture: wgpu::Texture,

    slider_pos: f32,
    slider_range: RangeInclusive<f32>,
    dragging_rn: bool,
}

struct AudioTrack {
    audio_ui: AudioUI,
}

struct VideoTrack {
    video_ui: VideoUI,
}

pub struct EditorGui {
    message_receiver: sync_mpsc::Receiver<WorkerMessage>,
    message_sender: tokio_mpsc::UnboundedSender<GUIMessage>,

    video_ui: VideoUI,
    audio_uis: HashMap<usize, AudioUI>,
}

impl EditorGui {
    pub fn new(
        cc: &CreationContext,
        context_oneshot: oneshot::Sender<egui::Context>,
        width: u32,
        height: u32,
        length: f32,
        message_receiver: sync_mpsc::Receiver<WorkerMessage>,
        message_sender: tokio_mpsc::UnboundedSender<GUIMessage>,
    ) -> Self {
        let (texture, texture_id) = if let Some(render_state) = &cc.wgpu_render_state {
            let device = &render_state.device;
            let mut renderer = render_state.renderer.write();
            let texture = textures::new_rgb_texture(device, width, height);
            let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
            (texture, renderer.register_native_texture(device, &view, FilterMode::Linear))
        } else {
            panic!("IDK")
        };

        let ctx = cc.egui_ctx.clone();
        context_oneshot.send(ctx).unwrap();

        let video_ui = VideoUI {
            texture_id,
            texture,
            slider_pos: 0.0,
            slider_range: 0.0..=length,
            dragging_rn: false,
        };

        let audio_uis = HashMap::new();

        Self {
            message_receiver,
            message_sender,
            video_ui,
            audio_uis,
        }
    }
}

impl eframe::App for EditorGui {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        while let Ok(worker_message) = self.message_receiver.try_recv() {
            match worker_message {
                WorkerMessage::Frame(video_frame, dur) => {
                    if let Some(dur) = dur {
                        self.video_ui.slider_pos += dur;
                    }
                    if let Some(render_state) = frame.wgpu_render_state() {
                        textures::write_into_texture(&self.video_ui.texture, self.video_ui.texture.width(), self.video_ui.texture.height(), &render_state.queue, video_frame);
                    }
                }
                WorkerMessage::AddAudioTrack(index) => {
                    self.audio_uis.insert(index,
                                          AudioUI {
                                              volume: 0.0,
                                              volume_range: 0.0..=4.,
                                          });
                }
            }
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.image((self.video_ui.texture_id.clone(), egui::vec2(self.video_ui.texture.width() as f32, self.video_ui.texture.height() as f32)));

            ui.group(|ui| {
                ui.horizontal(|ui| {
                    if ui.button("GLUTEN TAG").clicked() {
                        self.message_sender.send(GUIMessage::VideoStateChange(None)).unwrap();
                    }

                    let video_slider = ui.add(egui::Slider::new(&mut self.video_ui.slider_pos, self.video_ui.slider_range.clone())
                        .custom_formatter(|val, _| format!("{:.2}", val))
                        .text("SECONDS"));

                    if video_slider.drag_started() {
                        self.video_ui.dragging_rn = true;
                        self.message_sender.send(GUIMessage::VideoStateChange(Some(false))).unwrap();
                    }

                    if video_slider.drag_stopped() {
                        self.video_ui.dragging_rn = false;
                        self.message_sender.send(GUIMessage::VideoPosChanged(self.video_ui.slider_pos)).unwrap();
                        self.message_sender.send(GUIMessage::VideoStateChange(Some(true))).unwrap();
                    }
                    /*if !self.video_ui.dragging_rn {
                        self.video_ui.slider_pos = new idk!
                    }*/
                });
            });


            for (index, audio_ui) in &mut self.audio_uis {
                ui.group(|ui| {
                    ui.strong(format!("Audio {}", index));

                    let volume_slider = ui.add(egui::Slider::new(&mut audio_ui.volume, audio_ui.volume_range.clone())
                        .custom_formatter(|val, _| format!("{:.2}", val))
                        .text("VOLUME"));

                    if volume_slider.dragged() || volume_slider.drag_stopped() {
                        self.message_sender.send(GUIMessage::VolumeChanged(audio_ui.volume, *index)).unwrap();//TODO
                    }
                });
            }
        });
    }
}