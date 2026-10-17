use serde::{Deserialize, Serialize};

#[cfg(target_os = "linux")]
use crate::recorders::linux::{
    AudioEnvironment,
    VideoEnvironment,
};
#[cfg(target_os = "windows")]
use crate::recorders::windows::{
    AudioEnvironment,
    VideoEnvironment,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub frame_config: FrameConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "config", rename_all = "snake_case")]
pub enum FrameConfig {
    Video(VideoConfig),
    Audio(AudioConfig),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VideoConfig {
    pub fps: i32,
    pub width: u32,
    pub height: u32,
    pub environment: VideoEnvironment,
    pub codec: VideoCodec,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VideoCodec {
    HevcAmf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioConfig {
    pub environment: AudioEnvironment,
    pub codec: AudioCodec,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AudioCodec {
    Aac,
}