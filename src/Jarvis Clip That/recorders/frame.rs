use ffmpeg_next::ChannelLayout;
use ffmpeg_next::format::Sample;
use ffmpeg_next::frame::Audio;

use ffmpeg_next::sys::{av_buffer_ref, av_frame_alloc, av_hwframe_get_buffer, AVBufferRef, AVFrame, AVPixelFormat};
use crate::debug_println;
use crate::error::{CustomError, Error};

use crate::types::Result;

pub fn create_av_frame(
    format: AVPixelFormat,
    width: i32,
    height: i32,
    hw_frame_ctx: *mut AVBufferRef,
) -> Result<*mut AVFrame> {
    let av_frame;
    unsafe {
        av_frame = av_frame_alloc();
        if av_frame.is_null() {
            return Err(CustomError::CUSTOM(Error::Unknown));
        }
        (*av_frame).format = format as i32; // AV_PIX_FMT_D3D11 as i32;
        (*av_frame).width = width;
        (*av_frame).height = height;
        let hw_frames_ctx = av_buffer_ref(hw_frame_ctx);
        if hw_frames_ctx.is_null() {
            return Err(CustomError::CUSTOM(Error::Unknown));
        }
        (*av_frame).hw_frames_ctx = hw_frames_ctx;
    }
    let ret = unsafe {
        av_hwframe_get_buffer(hw_frame_ctx, av_frame, 0)
    };
    if ret < 0 || av_frame.is_null() {
        //return Err(format!("av_hwframe_get_buffer failed: {}", ret));
        return Err(CustomError::CUSTOM(Error::Unknown));
    }
    Ok(av_frame)
}

pub fn create_audio_frames(
    format: Sample,
    size: usize,
    layout: ChannelLayout,
) -> (Audio, Audio) {
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
    let buf = vec![0u8; size * silent_frame.format().bytes() * silent_frame.channels() as usize];
    debug_println!("SIZE: {}",size * silent_frame.format().bytes() * silent_frame.channels() as usize);
    unsafe { copy_into_audio_frame(&mut silent_frame, buf); }

    (frame, silent_frame)
}

pub unsafe fn copy_into_audio_frame(
    frame: &mut Audio,
    buffer: Vec<u8>,
) { // ONLY FOR 4 byte stereo
    let bytes_per_sample = frame.format().bytes();
    let num_channels = frame.channels() as usize;


    let linesize = unsafe { (*frame.as_ptr()).linesize[0] as usize };

    let planes: Vec<*mut u8> = (0..num_channels).map(|i| (*frame.as_ptr()).extended_data.add(i).read()).collect();

    let frame_size = num_channels * bytes_per_sample;
    let samples = buffer.len() / frame_size;

    for i in 0..samples {
        let frame_offset = i * frame_size;
        for channel_index in 0..num_channels {
            let chan_ptr = planes[channel_index];
            let dst = std::slice::from_raw_parts_mut(chan_ptr, linesize);

            let src_start = frame_offset + channel_index * bytes_per_sample;
            let src_end = src_start + bytes_per_sample;
            let dst_start = i * bytes_per_sample;
            let dst_end = dst_start + bytes_per_sample;

            dst[dst_start..dst_end].copy_from_slice(&buffer[src_start..src_end]);
        }
    }


    /*let ptr0 = unsafe { (*frame.as_ptr()).extended_data.offset(0).read() };
    let ptr1 = unsafe { (*frame.as_ptr()).extended_data.offset(1).read() };
    // get mutable slices to the destination planes
    let left_plane = unsafe { std::slice::from_raw_parts_mut(ptr0, linesize) };
    let right_plane = unsafe { std::slice::from_raw_parts_mut(ptr1, linesize) };

    // process buffer directly into planes
    for (i, chunk) in buffer.chunks(8).enumerate() {
        let offset = i * 4;
        left_plane[offset..offset + 4].copy_from_slice(&chunk[0..4]);
        right_plane[offset..offset + 4].copy_from_slice(&chunk[4..8]);
    }*/
}