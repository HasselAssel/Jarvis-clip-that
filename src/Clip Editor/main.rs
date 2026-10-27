#![cfg_attr(
    all(target_os = "windows", not(debug_assertions)),
    windows_subsystem = "windows"
)]

use crate::egui::GUI;

mod egui;
mod playback;
mod frame_converter;
mod wgpu;
#[path = "../macros.rs"]
mod macros;

#[tokio::main]
async fn main() {
    ffmpeg_next::init().unwrap_or_else(|e| panic!("ffmpeg_next init failed: {e}"));

    let _ = eframe::run_native(
        "Jarvis's Clip Editor",
        eframe::NativeOptions {
            renderer: eframe::Renderer::Wgpu,
            ..Default::default()
        },
        Box::new(|cc| Ok(Box::new(GUI::new(cc)))),
    );
}