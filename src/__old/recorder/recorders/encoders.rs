use ffmpeg_next::codec::encoder::find_by_name;
use ffmpeg_next::encoder::{audio, find, video};
use ffmpeg_next::sys::av_buffer_ref;
use ffmpeg_next::sys::AVBufferRef;

use crate::error::Error;
use crate::recorder::parameters::{AudioParams, VideoParams};
use crate::types::Result;


pub enum VideoFormatType {
    D3D11 { monitor_nr: u32 },
}

pub enum VideoFormatTypeData {
    D3D11 { hw_device_ctx: *mut AVBufferRef, hw_frame_ctx: *mut AVBufferRef },
}

pub enum VideoEncoderType {
    HevcAmf,
    QsvOderSo,
}


pub enum AudioFormatType {
    WasapiSystem,
    WasapiClient,
}

pub enum AudioFormatTypeData {
    WasapiSystem,
    WasapiClient { p_id: i32 },
}

#[repr(usize)]
pub enum AudioEncoderType { // represents the 'Frame Size'
    AAC = 1024,
}


pub fn new_video_encoder(video_params: &VideoParams, video_frame_type_data: &VideoFormatTypeData, video_encoder_type: &VideoEncoderType) -> Result<video::Encoder> {
    let codec = match video_encoder_type {
        VideoEncoderType::HevcAmf => {find_by_name("hevc_amf").ok_or(ffmpeg_next::Error::EncoderNotFound)?}
        VideoEncoderType::QsvOderSo => {return Err(Error::NotYetImplemented.into())}
    };

    let ctx = ffmpeg_next::codec::context::Context::new_with_codec(codec);
    let mut enc = ctx.encoder().video().unwrap();

    match video_frame_type_data {
        VideoFormatTypeData::D3D11 { hw_device_ctx, hw_frame_ctx } => {
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

pub fn new_audio_encoder(audio_params: &AudioParams, audio_frame_type_data: &AudioFormatTypeData, audio_encoder_type: &AudioEncoderType) -> Result<audio::Encoder> {
    let codec = match audio_encoder_type {
        AudioEncoderType::AAC => {find(ffmpeg_next::codec::Id::AAC).ok_or(ffmpeg_next::Error::EncoderNotFound)?}
        _ => return Err(Error::NotYetImplemented.into())
    };

    let ctx = ffmpeg_next::codec::context::Context::new_with_codec(codec);
    let mut enc = ctx.encoder().audio().unwrap();

    enc.set_rate(audio_params.base_params.rate);
    enc.set_channel_layout(audio_params.channel_layout);
    enc.set_format(audio_params.format);
    enc.set_time_base((1, audio_params.base_params.rate));
    enc.set_flags(audio_params.base_params.flags);

    let audio_encoder = enc.open_as(codec).unwrap();
    Ok(audio_encoder)
}