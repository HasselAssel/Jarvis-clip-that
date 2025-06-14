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

enum VideoFrameType {
    D3D11(ID3D11Device),
    Test,
}

enum VideoEncoderType {
    HevcAmf,
    QsvOderSo,
}

enum AudioFrameType {
}

enum AudioEncoderType {
}

pub fn new_video_encoder(video_params: &VideoParams, video_frame_type: &VideoFrameType, video_encoder_type: &VideoEncoderType) -> Result<video::Encoder> {
    let codec = match video_encoder_type {
        VideoEncoderType::HevcAmf => {find_by_name("hevc_amf").ok_or(ffmpeg_next::Error::EncoderNotFound)?}
        VideoEncoderType::QsvOderSo => {return Err(Error::NotYetImplemented.into())}
    };

    let ctx = ffmpeg_next::codec::context::Context::new_with_codec(codec);
    let mut enc = ctx.encoder().video().unwrap();

    match video_frame_type {
        VideoFrameType::D3D11(device) => {
            let raw_ctx = unsafe { enc.as_mut_ptr() };
            let (hw_device_ctx, hw_frame_ctx) = get_hw_device_and_frame_cxt(device, video_params);
            unsafe {
                (*raw_ctx).hw_device_ctx = av_buffer_ref(hw_device_ctx);
                (*raw_ctx).hw_frames_ctx = av_buffer_ref(hw_frame_ctx);
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

fn create_id3d11device(monitor: u32) -> Result<(ID3D11Device, ID3D11DeviceContext, IDXGIOutputDuplication)> {
    let mut device: Option<ID3D11Device> = None;
    let mut context: Option<ID3D11DeviceContext> = None;
    unsafe {
        D3D11CreateDevice(
            None,
            D3D_DRIVER_TYPE_HARDWARE,
            HMODULE::default(),
            D3D11_CREATE_DEVICE_BGRA_SUPPORT,
            Some(&[D3D_FEATURE_LEVEL_11_0, D3D_FEATURE_LEVEL_10_0]),
            D3D11_SDK_VERSION,
            Some(&mut device),
            None,
            Some(&mut context),
        )?;
    }
    let device: ID3D11Device = device?;
    let context: ID3D11DeviceContext = context?;

    let adapter: IDXGIAdapter;
    let output: IDXGIOutput;
    let duplication: IDXGIOutputDuplication;
    let out_desc: DXGI_OUTDUPL_DESC;

    let dxgi_device: IDXGIDevice = device.cast()?;
    unsafe {
        adapter = dxgi_device.GetAdapter()?;
        // Enumerate the first output (primary monitor).
        output = adapter.EnumOutputs(monitor)?;
    }
    let output1: IDXGIOutput1 = output.cast()?;
    unsafe {
        duplication = output1.DuplicateOutput(&device)?;
        out_desc = duplication.GetDesc();
    }
    Ok((device, context, duplication))
}

fn get_hw_device_and_frame_cxt(device: &ID3D11Device, video_params: &VideoParams) -> (*mut AVBufferRef, *mut AVBufferRef) {
    let mut hw_device_ctx = unsafe { av_hwdevice_ctx_alloc(AVHWDeviceType::AV_HWDEVICE_TYPE_D3D11VA) };
    if hw_device_ctx.is_null() {
        panic!("Failed to allocate HW device context");
    }

    #[repr(C)]
    pub struct AVD3D11VADeviceContext {
        pub device: *mut ID3D11Device,
    }

    let hwctx = unsafe {
        let device_ctx = (*hw_device_ctx).data as *mut ffmpeg_next::sys::AVHWDeviceContext;
        (*device_ctx).hwctx as *mut AVD3D11VADeviceContext
    };

    unsafe {
        (*hwctx).device = device.as_raw() as *mut _;
    }


    // Initialize the context
    let ret = unsafe { av_hwdevice_ctx_init(hw_device_ctx) };
    unsafe {
        if ret < 0 as _ {
            av_buffer_unref(&mut hw_device_ctx);
            panic!("Failed to initialize HW device context");
        }
    }


    let hw_frame_ctx: *mut AVBufferRef = unsafe { ffmpeg_next::sys::av_hwframe_ctx_alloc(hw_device_ctx) };
    if hw_frame_ctx.is_null() { panic!("alloc failed"); }

    let frames_ctx = unsafe { &mut *((*hw_frame_ctx).data as *mut ffmpeg_next::sys::AVHWFramesContext) };
    frames_ctx.format = ffmpeg_next::sys::AVPixelFormat::AV_PIX_FMT_D3D11;
    frames_ctx.sw_format = ffmpeg_next::sys::AVPixelFormat::AV_PIX_FMT_NV12;
    frames_ctx.width = video_params.out_width as i32;
    frames_ctx.height = video_params.out_height as i32;
    frames_ctx.initial_pool_size = 0;

    let ret = unsafe { av_hwframe_ctx_init(hw_frame_ctx) };
    if ret < 0 { panic!("init failed"); }

    (hw_device_ctx, hw_frame_ctx)
}