mod egui;
mod playback;
mod frame_converter;
mod wgpu;
#[path = "../macros.rs"]
mod macros;

use crate::egui::GUI;

#[tokio::main]
async fn main() {
    ffmpeg_next::init().unwrap_or_else(|e| panic!("ffmpeg_next init failed: {e}"));

    eframe::run_native(
        "Jarvis's Clip Editor",
        eframe::NativeOptions {
            renderer: eframe::Renderer::Wgpu,
            ..Default::default()
        },
        Box::new(|cc| Ok(Box::new(GUI::new(cc)))),
    );
}