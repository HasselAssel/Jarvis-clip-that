use crate::error::CustomError;

pub type Result<T> = std::result::Result<T, CustomError>;

pub type Packet = ffmpeg_next::Packet;