#[derive(Debug)]
pub enum IdkCustomErrorIGuess{
    FFMPEG(ffmpeg_next::Error),
    WINDOWS(windows::core::Error),
    WASAPI(wasapi::WasapiError),
}

impl From<ffmpeg_next::Error> for IdkCustomErrorIGuess {
    fn from(value: ffmpeg_next::Error) -> Self {
        IdkCustomErrorIGuess::FFMPEG(value)
    }
}

impl From<windows::core::Error> for IdkCustomErrorIGuess {
    fn from(value: windows::core::Error) -> Self {
        IdkCustomErrorIGuess::WINDOWS(value)
    }
}

impl From<wasapi::WasapiError> for IdkCustomErrorIGuess {
    fn from(value: wasapi::WasapiError) -> Self {
        IdkCustomErrorIGuess::WASAPI(value)
    }
}