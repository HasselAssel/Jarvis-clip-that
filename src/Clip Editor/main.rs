mod egui;
mod playback;
mod frame_converter;
mod wgpu;

fn main() {
    ffmpeg_next::init().unwrap_or_else(|e| panic!("ffmpeg_next init failed: {e}"));
}