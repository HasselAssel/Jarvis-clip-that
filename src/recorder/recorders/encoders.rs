use std::ptr::null_mut;
use ffmpeg_next::codec::encoder::find_by_name;
use ffmpeg_next::encoder::{video, audio};
use ffmpeg_next::ffi::{av_buffer_unref, av_hwdevice_ctx_alloc, av_hwdevice_ctx_init, av_hwframe_ctx_init, AVHWDeviceType};
use ffmpeg_next::sys::av_buffer_ref;
use ffmpeg_next::sys::AVBufferRef;
use windows::Win32::Foundation::HMODULE;
use windows::Win32::Graphics::Direct3D11::{D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_SDK_VERSION, D3D11CreateDevice, ID3D11Device, ID3D11DeviceContext};
use windows::Win32::Graphics::Direct3D::{D3D_DRIVER_TYPE_HARDWARE, D3D_FEATURE_LEVEL_10_0, D3D_FEATURE_LEVEL_11_0};
use windows::Win32::Graphics::Dxgi::{DXGI_OUTDUPL_DESC, IDXGIAdapter, IDXGIDevice, IDXGIOutput, IDXGIOutput1, IDXGIOutputDuplication};
use windows_core::Interface;

use crate::recorder::parameters::{AudioParams, VideoParams};
use crate::types::Result;
use crate::error::Error;
use crate::recorder::recorders::d3d11::get_hw_device_and_frame_cxt;

pub enum VideoFrameType {
    D3D11 { hw_device_ctx: *mut AVBufferRef, hw_frame_ctx: *mut AVBufferRef },
    Test,
}

pub enum VideoEncoderType {
    HevcAmf,
    QsvOderSo,
}

pub enum AudioFrameType {
}

pub enum AudioEncoderType {
}

pub fn new_video_encoder(video_params: &VideoParams, video_frame_type: &VideoFrameType, video_encoder_type: &VideoEncoderType) -> Result<video::Encoder> {
    let codec = match video_encoder_type {
        VideoEncoderType::HevcAmf => {find_by_name("hevc_amf").ok_or(ffmpeg_next::Error::EncoderNotFound)?}
        VideoEncoderType::QsvOderSo => {return Err(Error::NotYetImplemented.into())}
    };

    let ctx = ffmpeg_next::codec::context::Context::new_with_codec(codec);
    let mut enc = ctx.encoder().video().unwrap();

    match video_frame_type {
        VideoFrameType::D3D11 { hw_device_ctx, hw_frame_ctx } => {
            let raw_ctx = unsafe { enc.as_mut_ptr() };
            unsafe {
                (*raw_ctx).hw_device_ctx = av_buffer_ref(*hw_device_ctx);
                (*raw_ctx).hw_frames_ctx = av_buffer_ref(*hw_frame_ctx);
            }
            enc.set_width(video_params.out_width);
            enc.set_height(video_params.out_height);
            enc.set_format(ffmpeg_next::format::Pixel::D3D11);
            enc.set_time_base((1, video_params.base_params.rate));
            enc.set_frame_rate(Some((video_params.base_params.rate, 1)));
            enc.set_bit_rate(video_params.base_params.bit_rate);
            enc.set_max_bit_rate(video_params.base_params.max_bit_rate);
            enc.set_flags(video_params.base_params.flags);
            enc.set_gop(video_params.base_params.rate as u32); // Keyframe interval (1 second)

            let video_encoder = enc.open_as(codec)?;
            Ok(video_encoder)
        }
        _ => return Err(Error::NotYetImplemented.into())
    }
}

pub fn new_audio(audio_params: AudioParams, audio_frame_type: AudioFrameType, audio_encoder_type: AudioEncoderType) -> audio::Encoder {
    todo!()
}