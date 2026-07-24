use std::ptr::null_mut;

use anyhow::{bail, Context, Result};
use ffmpeg_next::codec::{codec, Parameters};
use ffmpeg_next::codec::encoder;
use ffmpeg_next::codec::Flags;
use ffmpeg_next::ffi::{av_buffer_create, av_buffer_ref, av_buffer_unref, av_frame_alloc, av_hwdevice_ctx_alloc, av_hwdevice_ctx_init, av_hwframe_ctx_alloc, av_hwframe_ctx_init, av_hwframe_get_buffer, AVBufferRef, AVFrame, AVHWDeviceContext, AVHWDeviceType, AVHWFramesContext, AVPixelFormat};
use ffmpeg_next::Packet;
use ffmpeg_next::sys::AVPixelFormat::AV_PIX_FMT_D3D11;
use ffmpeg_next::util::frame::video;
use windows::Win32::Graphics::Direct3D11::{ID3D11Device, ID3D11Texture2D};
use windows_core::Interface;

use crate::recorders::env::EnvD3D11;
use crate::recorders::traits::Encoder;

pub struct EncoderD3D11 {
    encoder: encoder::video::Encoder,
    av_frame: *mut AVFrame,
    frame: video::Video,
}

impl EncoderD3D11 {
    pub fn new(
        (width, height): (u32, u32),
        fps: i32,
        enc: encoder::video::Video,
        codec: codec::Codec,
        env: <Self as Encoder>::Env<'_>,
    ) -> Result<Self> {
        let i_width = i32::try_from(width).with_context(|| format!("height exceeds i32::MAX: {width}"))?;
        let i_height = i32::try_from(height).with_context(|| format!("height exceeds i32::MAX: {height}"))?;
        let (hw_device_ctx, hw_frame_ctx) = setup_hw_and_frame_ctx(&env.device, i_width, i_height)?;
        let encoder = create_encoder_d3d11(
            enc,
            codec,
            (Some(hw_device_ctx), hw_frame_ctx),
            width,
            height,
            fps
        )?;
        let av_frame = create_av_frame(AV_PIX_FMT_D3D11, i_width, i_height, hw_frame_ctx)?;
        let frame = unsafe { video::Video::wrap(av_frame) };

        Ok(Self {
            encoder,
            av_frame,
            frame,
        })
    }

    pub fn get_params(&self) -> Parameters {
        Parameters::from(&self.encoder)
    }
}

impl Encoder for EncoderD3D11 {
    type Env<'e> = &'e EnvD3D11;
    type Input<'i> = ID3D11Texture2D;
    type Output<'o> = Vec<Packet>;

    fn encode(&mut self, input: Self::Input<'_>, _: Self::Env<'_>) -> Result<Self::Output<'_>> {
        insert_texture_into_frame(self.av_frame, &input)?;

        self.encoder.send_frame(&self.frame)?;

        let mut packet_vec = Vec::with_capacity(2);
        let mut packet = Packet::empty();
        while self.encoder.receive_packet(&mut packet).is_ok() {
            let packet_clone = packet.clone();
            //packet_clone.set_duration(duration);
            packet_vec.push(packet_clone);
        }

        Ok(packet_vec)
    }
}

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
            bail!("av_frame is null (1)");
        }
        (*av_frame).format = format as i32;
        (*av_frame).width = width;
        (*av_frame).height = height;
        let hw_frames_ctx = av_buffer_ref(hw_frame_ctx);
        if hw_frames_ctx.is_null() {
            bail!("hw_frames_ctx is null");
        }
        (*av_frame).hw_frames_ctx = hw_frames_ctx;
    }
    let ret = unsafe { av_hwframe_get_buffer(hw_frame_ctx, av_frame, 0) };
    if ret < 0 || av_frame.is_null() {
        bail!("av_frame is null (2)");
    }
    Ok(av_frame)
}

fn insert_texture_into_frame(
    av_frame: *mut AVFrame,
    texture: &ID3D11Texture2D,
) -> Result<()> {
    let texture_buffer = unsafe {
        av_buffer_create(
            texture.as_raw() as _,
            size_of::<*mut ID3D11Texture2D>(),
            None, //Some(Self::buffer_free),
            null_mut(),
            0,
        )
    };
    if texture_buffer.is_null() {
        bail!("cant create buffer");
    }
    unsafe {
        (*av_frame).data[0] = texture.as_raw() as _;
        (*av_frame).buf[0] = texture_buffer;
    }

    Ok(())
}

fn setup_hw_and_frame_ctx(
    device: &ID3D11Device,
    width: i32,
    height: i32,
) -> Result<(*mut AVBufferRef, *mut AVBufferRef)> {
    let mut hw_device_ctx =
        unsafe { av_hwdevice_ctx_alloc(AVHWDeviceType::AV_HWDEVICE_TYPE_D3D11VA) };
    if hw_device_ctx.is_null() {
        bail!("Failed to allocate HW device context")
    }

    #[repr(C)]
    pub struct AVD3D11VADeviceContext {
        pub device: *mut ID3D11Device,
    }

    let hwctx = unsafe {
        let device_ctx = (*hw_device_ctx).data as *mut AVHWDeviceContext;
        (*device_ctx).hwctx as *mut AVD3D11VADeviceContext
    };

    unsafe {
        (*hwctx).device = device.as_raw() as *mut _;
    }

    let ret = unsafe { av_hwdevice_ctx_init(hw_device_ctx) };

    if ret < 0 as _ {
        unsafe {
            av_buffer_unref(&mut hw_device_ctx);
            panic!("Failed to initialize HW device context");
        }
    }

    let hw_frame_ctx: *mut AVBufferRef = unsafe { av_hwframe_ctx_alloc(hw_device_ctx) };
    if hw_frame_ctx.is_null() {
        panic!("alloc failed");
    }

    let frames_ctx = unsafe { &mut *((*hw_frame_ctx).data as *mut AVHWFramesContext) };
    frames_ctx.format = AVPixelFormat::AV_PIX_FMT_D3D11;
    frames_ctx.sw_format = AVPixelFormat::AV_PIX_FMT_NV12;
    frames_ctx.width = width;
    frames_ctx.height = height;
    frames_ctx.initial_pool_size = 0;

    let ret = unsafe { av_hwframe_ctx_init(hw_frame_ctx) };
    if ret < 0 {
        panic!("init failed");
    }

    Ok((hw_device_ctx, hw_frame_ctx))
}

pub fn create_encoder_d3d11(
    mut enc: ffmpeg_next::codec::encoder::video::Video,
    codec: codec::Codec,
    (hw_device_ctx, hw_frame_ctx): (Option<*mut AVBufferRef>, *mut AVBufferRef),
    width: u32,
    height: u32,
    fps: i32,
) -> Result<ffmpeg_next::encoder::video::Encoder> {
    let raw_ctx = unsafe { enc.as_mut_ptr() };
    if raw_ctx.is_null() {
        bail!("raw_ctx is null");
    }
    unsafe {
        if let Some(hw_device_ctx) = hw_device_ctx {
            let hw_device_ctx = av_buffer_ref(hw_device_ctx);
            if hw_device_ctx.is_null() {
                bail!("hw_device_ctx is null");
            }
            (*raw_ctx).hw_device_ctx = hw_device_ctx;
        }
        let hw_frames_ctx = av_buffer_ref(hw_frame_ctx);
        if hw_frames_ctx.is_null() {
            bail!("hw_frames_ctx is null");
        }
        (*raw_ctx).hw_frames_ctx = hw_frames_ctx;
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
