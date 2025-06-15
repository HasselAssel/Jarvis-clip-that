use std::ptr::null_mut;

use ffmpeg_next::sys::{av_buffer_ref, av_frame_alloc, av_hwframe_get_buffer, AVBufferRef, AVFrame, AVPixelFormat};

use crate::error::Error::AvFrameCreationFailed;
use crate::types::Result;

pub fn create_av_frame(format: AVPixelFormat, width: i32, height: i32, hw_frame_ctx: *mut AVBufferRef) -> Result<*mut AVFrame> {
    let mut av_frame = null_mut();
    unsafe {
        av_frame = av_frame_alloc();
        (*av_frame).format = format as i32; // AV_PIX_FMT_D3D11 as i32;
        (*av_frame).width = width;
        (*av_frame).height = height;
        (*av_frame).hw_frames_ctx = av_buffer_ref(hw_frame_ctx);
    }
    let ret = unsafe {
        av_hwframe_get_buffer(hw_frame_ctx, av_frame, 0)
    };
    if ret < 0 || av_frame.is_null() {
        //return Err(format!("av_hwframe_get_buffer failed: {}", ret));
        return Err(AvFrameCreationFailed.into())
    }
    /*unsafe {
        (*av_frame).format = format as i32; // AV_PIX_FMT_D3D11 as i32;
        (*av_frame).width = self.out_width as _;
        (*av_frame).height = self.out_height as _;
        //(*av_frame).data[0] = nv12_tex.as_raw() as _;
        //(*av_frame).buf[0] = texture_buffer;
        (*av_frame).hw_frames_ctx = av_buffer_ref(hw_frame_ctx as _);
    }*/
    Ok(av_frame)
}