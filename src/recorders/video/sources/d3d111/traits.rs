use ffmpeg_next::Codec;
use ffmpeg_next::codec::encoder::video::Video;
use ffmpeg_next::codec::Flags;
use ffmpeg_next::encoder::video::Encoder;
use ffmpeg_next::sys::{av_buffer_ref, AVBufferRef, AVFrame};
use windows::Win32::Graphics::Direct3D11::{ID3D11Device, ID3D11Texture2D};
use crate::wrappers::MaybeSafeFFIPtrWrapper;
use crate::types::Result;

pub trait D3d11EncoderHwContext {
    fn setup_hw_and_frame_ctx(&self, device: &ID3D11Device, width: i32, height: i32) -> Result<(Option<*mut AVBufferRef>, *mut AVBufferRef)>;
    fn prepare_frame(&self, av_frame: &MaybeSafeFFIPtrWrapper<AVFrame>, texture: &ID3D11Texture2D) -> Result<()>;
}

pub fn create_encoder_d3d11(mut enc: Video, codec: Codec, (hw_device_ctx, hw_frame_ctx): (Option<*mut AVBufferRef>, *mut AVBufferRef), width: u32, height: u32, fps: i32) -> Result<Encoder> {
    let raw_ctx = unsafe { enc.as_mut_ptr() };
    unsafe {
        if let Some(hw_device_ctx) = hw_device_ctx {
            (*raw_ctx).hw_device_ctx = av_buffer_ref(hw_device_ctx);
        }
        (*raw_ctx).hw_frames_ctx = av_buffer_ref(hw_frame_ctx);
    }
    enc.set_width(width);
    enc.set_height(height);
    enc.set_format(ffmpeg_next::format::Pixel::D3D11);
    enc.set_time_base((1, fps));
    enc.set_frame_rate(Some((fps, 1)));
    enc.set_bit_rate(8_000_000);
    enc.set_max_bit_rate(10_000_000);
    enc.set_flags(Flags::GLOBAL_HEADER);
    enc.set_gop(fps as u32); // Keyframe interval (1 second)

    let video_encoder = enc.open_as(codec)?;
    Ok(video_encoder)
}