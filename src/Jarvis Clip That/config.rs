use std::{fs, path::Path};

use anyhow::{Context, Result};
use rdev::Key;
use serde::{Deserialize, Serialize};

use crate::recorders::config::{AudioCodec, Config};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub retention_secs: i64,
    pub output_dir: String,
    pub recorders: Vec<Config>,
    pub watch_processes: Option<(AudioCodec, bool)>,
    pub keybinds: Keybinds,
}

impl AppConfig {
    pub fn load(path: impl AsRef<Path>) -> Result<Self> {
        let content = fs::read_to_string(path.as_ref())
            .context("Failed to read config file")?;

        toml::from_str(&content)
            .context("Failed to parse config file")
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Keybinds {
    pub save_clip: Vec<Vec<Key>>,
    pub save_clip_sound_file: Option<String>,
    pub save_clip_sound_volume: Option<f32>,
}