use std::ptr::{null, null_mut};
use ffmpeg_next::ffi::{AVBufferRef, AVFrame};
use ffmpeg_next::sys::{av_hwdevice_ctx_create, av_hwdevice_ctx_create_derived, av_buffer_unref, av_hwframe_ctx_init, av_hwframe_ctx_alloc, AVHWDeviceType, AVHWFramesContext, AVPixelFormat};
use windows::Win32::Graphics::Direct3D11::{ID3D11Device, ID3D11Texture2D};
use crate::error::Error::Unknown;
use crate::recorders::video::sources::d3d111::traits::D3d11EncoderHwContext;
use crate::wrappers::MaybeSafeFFIPtrWrapper;
use crate::types::Result;

pub struct QsvAdapter;

impl D3d11EncoderHwContext for QsvAdapter {
    fn setup_hw_and_frame_ctx(&self, _device: &ID3D11Device, width: i32, height: i32) -> Result<(Option<*mut AVBufferRef>, *mut AVBufferRef)> {
        let mut d3d11_hwdev: *mut AVBufferRef = null_mut();
        let mut qsv_hwdev: *mut AVBufferRef = null_mut();

        // 1️⃣ Create D3D11 device context for QSV interop
        let ret = unsafe {
            av_hwdevice_ctx_create(
                &mut d3d11_hwdev,
                AVHWDeviceType::AV_HWDEVICE_TYPE_D3D11VA,
                null(),
                null_mut(),
                0,
            )
        };
        if ret < 0 || d3d11_hwdev.is_null() {
            return Err(Unknown.into());
        }

        // 2️⃣ Derive QSV device context from D3D11
        let ret = unsafe {
            av_hwdevice_ctx_create_derived(
                &mut qsv_hwdev,
                AVHWDeviceType::AV_HWDEVICE_TYPE_QSV,
                d3d11_hwdev,
                0,
            )
        };
        if ret < 0 || qsv_hwdev.is_null() {
            unsafe { av_buffer_unref(&mut d3d11_hwdev); }
            return Err(Unknown.into());
        }

        // 3️⃣ Allocate a frames context for QSV
        let mut hw_frame_ctx = unsafe { av_hwframe_ctx_alloc(qsv_hwdev) };
        if hw_frame_ctx.is_null() {
            unsafe {
                av_buffer_unref(&mut d3d11_hwdev);
                av_buffer_unref(&mut qsv_hwdev);
            }
            return Err(Unknown.into());
        }

        let frames_ctx = unsafe { &mut *((*hw_frame_ctx).data as *mut AVHWFramesContext) };
        frames_ctx.format = AVPixelFormat::AV_PIX_FMT_QSV;
        frames_ctx.sw_format = AVPixelFormat::AV_PIX_FMT_NV12;
        frames_ctx.width = width;
        frames_ctx.height = height;
        frames_ctx.initial_pool_size = 0; // 16

        let ret = unsafe { av_hwframe_ctx_init(hw_frame_ctx) };
        if ret < 0 {
            unsafe {
                av_buffer_unref(&mut d3d11_hwdev);
                av_buffer_unref(&mut qsv_hwdev);
                av_buffer_unref(&mut hw_frame_ctx);
            }
            return Err(Unknown.into());
        }

        // 4️⃣ QSV context now holds reference to D3D11 device internally
        // We can unref the original D3D11 device buffer safely
        unsafe { av_buffer_unref(&mut d3d11_hwdev) };

        Ok((Some(qsv_hwdev), hw_frame_ctx))
    }

    fn prepare_frame(&self, _av_frame: &MaybeSafeFFIPtrWrapper<AVFrame>, _texture: &ID3D11Texture2D) -> Result<()> {
        todo!()
    }
}