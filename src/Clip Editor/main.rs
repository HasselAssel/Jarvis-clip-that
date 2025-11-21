use crate::editor::ClipEditor;
use crate::media_playback::{AudioSettings, VideoSettings};

#[path = "../shared_macros.rs"]
mod shared_macros;
mod media;
mod egui;
mod stream;
mod media_playback;
mod decoders;
mod stream_scheduler;
mod textures;
mod editor;
mod audio_playback;

fn main() {
    let video_settings = VideoSettings {
        width: 1000,
        height: 750,
    };
    let audio_setting = AudioSettings;

    let ce = ClipEditor::new(video_settings, audio_setting);
    ce.start_gui();
}