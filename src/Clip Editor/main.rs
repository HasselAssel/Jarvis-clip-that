mod egui;
mod playback;

fn main() {
    ffmpeg_next::init().unwrap_or_else(|e| panic!("ffmpeg_next init failed: {e}"));
}