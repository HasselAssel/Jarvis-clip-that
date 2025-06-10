#[derive(Debug)]
pub enum CustomError {
    FFMPEG(ffmpeg_next::Error),
    WINDOWS(windows::core::Error),
    WASAPI(wasapi::WasapiError),
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