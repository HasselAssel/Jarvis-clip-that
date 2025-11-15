use crate::clip_editor::ClipEditor;

#[path = "../shared_macros.rs"]
mod shared_macros;
mod egui;
mod media_decoder;
mod media;
mod stream_handle;
mod decoders;
mod textures;
mod media_playback;
mod clip_editor;
mod audio_playback;


pub fn main() {
    let ce = ClipEditor::new();
    ce.start_gui();
}