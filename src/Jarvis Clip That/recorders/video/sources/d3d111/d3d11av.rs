use std::ptr::null_mut;
use ffmpeg_next::ffi::{av_buffer_create, AVBufferRef, AVFrame};
use ffmpeg_next::sys::{av_hwdevice_ctx_alloc, av_hwdevice_ctx_init, av_hwframe_ctx_alloc, av_hwframe_ctx_init, av_buffer_unref, AVHWDeviceType, AVHWDeviceContext, AVPixelFormat, AVHWFramesContext};
use windows::core::Interface;
use windows::Win32::Graphics::Direct3D11::{ID3D11Device, ID3D11Texture2D};
use crate::error::{CustomError, Error};
use crate::recorders::video::sources::d3d111::traits::D3d11EncoderHwContext;
use crate::wrappers::MaybeSafeFFIPtrWrapper;
use crate::types::Result;

pub struct D3d11vaAdapter;

impl D3d11EncoderHwContext for D3d11vaAdapter {
    fn setup_hw_and_frame_ctx(
        &self, device: &ID3D11Device,
        width: i32,
        height: i32,
    ) -> Result<(Option<*mut AVBufferRef>, *mut AVBufferRef)> {
        let mut hw_device_ctx = unsafe { av_hwdevice_ctx_alloc(AVHWDeviceType::AV_HWDEVICE_TYPE_D3D11VA) };
        if hw_device_ctx.is_null() { //"Failed to allocate HW device context"
            return Err(CustomError::CUSTOM(Error::Unknown));
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


        // Initialize the context
        let ret = unsafe { av_hwdevice_ctx_init(hw_device_ctx) };

        if ret < 0 as _ {
            unsafe {
                av_buffer_unref(&mut hw_device_ctx);
                panic!("Failed to initialize HW device context");
            }
        }


        let hw_frame_ctx: *mut AVBufferRef = unsafe { av_hwframe_ctx_alloc(hw_device_ctx) };
        if hw_frame_ctx.is_null() { panic!("alloc failed"); }

        let frames_ctx = unsafe { &mut *((*hw_frame_ctx).data as *mut AVHWFramesContext) };
        frames_ctx.format = AVPixelFormat::AV_PIX_FMT_D3D11;
        frames_ctx.sw_format = AVPixelFormat::AV_PIX_FMT_NV12;
        frames_ctx.width = width;
        frames_ctx.height = height;
        frames_ctx.initial_pool_size = 0;

        let ret = unsafe { av_hwframe_ctx_init(hw_frame_ctx) };
        if ret < 0 { panic!("init failed"); }

        Ok((Some(hw_device_ctx), hw_frame_ctx))
    }

    fn prepare_frame(
        &self,
        av_frame: &MaybeSafeFFIPtrWrapper<AVFrame>,
        texture: &ID3D11Texture2D,
    ) -> Result<()> {
        let texture_buffer = unsafe {
            av_buffer_create(
                texture.as_raw() as _,
                size_of::<*mut ID3D11Texture2D>(),
                None,//Some(Self::buffer_free),
                null_mut(),
                0,
            )
        };
        if texture_buffer.is_null() {
            return Err(CustomError::CUSTOM(Error::Unknown));
        }
        unsafe {
            (***av_frame).data[0] = texture.as_raw() as _;
            (***av_frame).buf[0] = texture_buffer;
        }

        Ok(())
    }
}