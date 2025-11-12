pub type Result<T> = std::result::Result<T, crate::error::CustomError>;

pub type Packet = ffmpeg_next::Packet;

pub type RecorderJoinHandle = std::thread::JoinHandle<Result<()>>;