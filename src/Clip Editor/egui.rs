use std::io::Write;
use std::sync::mpsc as sync_mpsc;
use tokio::sync::mpsc as tokio_mpsc;

use eframe::{CreationContext, egui, wgpu};
use eframe::epaint::TextureId;
use eframe::wgpu::FilterMode;
use ffmpeg_next::frame;
use tokio::sync::oneshot;

use crate::textures;

pub enum GUIMessage {

}

pub enum WorkerMessage {
    Frame(frame::Video)
}



pub struct EditorGui {
    message_receiver: sync_mpsc::Receiver<WorkerMessage>,
    message_sender: tokio_mpsc::UnboundedSender<GUIMessage>,
    texture_id: TextureId,
    texture: wgpu::Texture,
}

impl EditorGui {
    pub fn new(cc: &CreationContext, context_oneshot: oneshot::Sender<egui::Context>, width: u32, height: u32, message_receiver: sync_mpsc::Receiver<WorkerMessage>, message_sender: tokio_mpsc::UnboundedSender<GUIMessage>) -> Self {
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

        Self {
            message_receiver,
            message_sender,
            texture_id,
            texture,
        }
    }
}

impl eframe::App for EditorGui {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        if let Ok(worker_message) = self.message_receiver.try_recv() {
            if let WorkerMessage::Frame(video_frame) = worker_message {
                if let Some(render_state) = frame.wgpu_render_state() {
                    textures::write_into_texture(&self.texture, self.texture.width(), self.texture.height(), &render_state.queue, video_frame);
                }
            }
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.image((self.texture_id.clone(), egui::vec2(self.texture.width() as f32, self.texture.height() as f32)));
        });
    }
}