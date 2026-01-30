use rdev::Key;
use serde::Deserialize;
use crate::recorders::audio::sources::enums::{AudioCodec, AudioSourceType};
use crate::recorders::video::sources::enums::{VideoCodec, VideoSourceType};
use crate::types::Result;

#[derive(Deserialize)]
struct Config {
    save: SaveConfig,
    recorder: RecorderConfig,
}

#[derive(Deserialize)]
struct SaveConfig {
    #[serde(default = "default_shortcuts")]
    shortcuts: Vec<Vec<Key>>,
    save_dir: String,
    base_file_name: String,
    sound_file: Option<String>,
}

struct RecorderConfig {
    video_source_type: VideoSourceType,
    video_codec: VideoCodec,
    audio_source_type: AudioSourceType,
    audio_codec: AudioCodec,

    max_seconds: u32,
    fps: i32
}

pub fn parse_config() -> Result<Config> {
    let text = std::fs::read_to_string("config.toml")?;
    Ok(toml::from_str(&text)?)
}


fn default_shortcuts() -> Vec<Vec<Key>> {
    vec![vec![Key::Alt, Key::KeyM]]
}