use ffmpeg_next::ffi::AVFrame;
use crate::wrappers::MaybeSafeFFIPtrWrapper;
use crate::types::Result;

pub trait VideoSource {
    fn init(&mut self) -> Result<()>;
    fn get_frame(&mut self, av_frame: &MaybeSafeFFIPtrWrapper<AVFrame>, out_width: u32, out_height: u32) -> Result<()>;
}