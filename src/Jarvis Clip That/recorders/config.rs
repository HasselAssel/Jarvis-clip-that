#[cfg(target_os = "windows")]
use crate::recorders::windows::{
    VideoEnvironment,
    AudioEnvironment,
};
#[cfg(target_os = "linux")]
use crate::recorders::linux::{
    VideoEnvironment,
    AudioEnvironment,
};

pub struct VideoConfig {
    pub fps: i32,
    pub width: u32,
    pub height: u32,
    pub environment: VideoEnvironment,
    pub codec: Codec,
}

pub struct AudioConfig {
    pub environment: AudioEnvironment,
    pub codec: Codec,
}

pub enum Codec {
    HevcAmf,
}