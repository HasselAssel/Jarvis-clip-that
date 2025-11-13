#[derive(Debug)]
pub enum Error {
    jrNotYetImplemented,
    NonExistentParameterCombination,

    Unknown,
}

#[allow(dead_code)]
#[derive(Debug)]
pub enum CustomError {
    FFMPEG(ffmpeg_next::Error),
    WINDOWS(windows::core::Error),
    IO(std::io::Error),
    CUSTOM(Error),
}

impl From<ffmpeg_next::Error> for CustomError {
    fn from(value: ffmpeg_next::Error) -> Self {
        CustomError::FFMPEG(value)
    }
}

impl From<windows::core::Error> for CustomError {
    fn from(value: windows::core::Error) -> Self {
        CustomError::WINDOWS(value)
    }
}

impl From<std::io::Error> for CustomError {
    fn from(value: std::io::Error) -> Self {
        CustomError::IO(value)
    }
}


impl From<Error> for CustomError {
    fn from(value: Error) -> Self {
        CustomError::CUSTOM(value)
    }
}