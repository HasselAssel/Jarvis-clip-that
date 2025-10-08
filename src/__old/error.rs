#[derive(Debug)]
pub enum Error {
    NotYetImplemented,
    NonExistentParameterCombination,

    Unknown,
}

#[derive(Debug)]
pub enum CustomError {
    FFMPEG(ffmpeg_next::Error),
    WINDOWS(windows::core::Error),
    WASAPI(wasapi::WasapiError),
    IO(std::io::Error),

    CUSTOM(Error)
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

impl From<wasapi::WasapiError> for CustomError {
    fn from(value: wasapi::WasapiError) -> Self {
        CustomError::WASAPI(value)
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