use crate::editor::ClipEditor;
use crate::media_playback::AudioSettings;
use crate::media_playback::VideoSettings;

mod audio_playback;
mod decoders;
mod edit;
mod editor;
mod egui;
mod media;
mod media_playback;
#[path = "../shared_macros.rs"]
mod shared_macros;
mod stream;
mod stream_scheduler;
mod textures;

fn main() {
    let video_settings = VideoSettings {
        width: 1000,
        height: 750,
    };
    let audio_setting = AudioSettings { initial_vol: 1. };

    let ce = ClipEditor::new(video_settings, audio_setting);
    ce.start_gui();
}
