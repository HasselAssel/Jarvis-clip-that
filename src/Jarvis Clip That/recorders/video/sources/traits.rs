use ffmpeg_next::ffi::AVFrame;
use crate::wrappers::MaybeSafeFFIPtrWrapper;

pub trait VideoSource {
    fn init(&mut self);
    fn get_frame(&mut self, av_frame: &MaybeSafeFFIPtrWrapper<AVFrame>, out_width: u32, out_height: u32) -> Result<(), String>;
}