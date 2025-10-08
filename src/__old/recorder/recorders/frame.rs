use std::ptr::null_mut;
use ffmpeg_next::ChannelLayout;
use ffmpeg_next::format::Sample;
use ffmpeg_next::frame::Audio;

use ffmpeg_next::sys::{av_buffer_ref, av_frame_alloc, av_hwframe_get_buffer, AVBufferRef, AVFrame, AVPixelFormat};
use windows_core::s;

use crate::error::Error::Unknown;
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
        return Err(Unknown.into());
    }
    Ok(av_frame)
}

pub fn create_frames(format: Sample, size: usize, layout: ChannelLayout) -> (Audio, Audio) {
    let frame = Audio::new(
        format,
        size,
        layout,
    );
    let mut silent_frame = Audio::new(
        format,
        size,
        layout,
    );
    let buf = vec![0u8; size * silent_frame.format().bytes()];
    unsafe { copy_into_audio_frame(&mut silent_frame, buf); }

    (frame, silent_frame)
}

pub unsafe fn copy_into_audio_frame(frame: &mut Audio, buffer: Vec<u8>) { //ONLY FOR 4 byte samples
    let linesize = unsafe { (*frame.as_ptr()).linesize[0] as usize };
    let ptr0 = unsafe { (*frame.as_ptr()).extended_data.offset(0).read() };
    let ptr1 = unsafe { (*frame.as_ptr()).extended_data.offset(1).read() };
    // Get mutable slices to the destination planes first
    let left_plane = unsafe { std::slice::from_raw_parts_mut(ptr0, linesize) };
    let right_plane = unsafe { std::slice::from_raw_parts_mut(ptr1, linesize) };

    // Process buffer directly into planes
    for (i, chunk) in buffer.chunks(8).enumerate() {
        let offset = i * 4;
        left_plane[offset..offset + 4].copy_from_slice(&chunk[0..4]);
        right_plane[offset..offset + 4].copy_from_slice(&chunk[4..8]);
    }

    /*for (i, chunk) in buffer.chunks(8).enumerate() {
        if chunk.len() >= 8 {
            let offset = i * 4;
            if offset + 4 > left_plane.len() || offset + 4 > right_plane.len() {
                panic!("Destination planes too small");
            }
            left_plane[offset..offset + 4].copy_from_slice(&chunk[0..4]);
            right_plane[offset..offset + 4].copy_from_slice(&chunk[4..8]);
        } else {
            panic!("Data not divisible by 8");
        }
    }*/
}