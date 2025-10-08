use std::mem::ManuallyDrop;

use ffmpeg_next::ffi::{av_buffer_unref, av_hwdevice_ctx_alloc, av_hwdevice_ctx_init, av_hwframe_ctx_init, AVBufferRef, AVHWDeviceType};
use windows::Win32::Foundation::{HMODULE, TRUE};
use windows::Win32::Graphics::Direct3D::{D3D_DRIVER_TYPE_HARDWARE, D3D_FEATURE_LEVEL_10_0, D3D_FEATURE_LEVEL_11_0};
use windows::Win32::Graphics::Direct3D11::{D3D11_BIND_RENDER_TARGET, D3D11_BIND_SHADER_RESOURCE, D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_SDK_VERSION, D3D11_TEX2D_VPIV, D3D11_TEX2D_VPOV, D3D11_TEXTURE2D_DESC, D3D11_USAGE_DEFAULT, D3D11_VIDEO_FRAME_FORMAT_PROGRESSIVE, D3D11_VIDEO_PROCESSOR_CONTENT_DESC, D3D11_VIDEO_PROCESSOR_INPUT_VIEW_DESC, D3D11_VIDEO_PROCESSOR_INPUT_VIEW_DESC_0, D3D11_VIDEO_PROCESSOR_OUTPUT_VIEW_DESC, D3D11_VIDEO_PROCESSOR_OUTPUT_VIEW_DESC_0, D3D11_VIDEO_PROCESSOR_STREAM, D3D11_VPIV_DIMENSION_TEXTURE2D, D3D11_VPOV_DIMENSION_TEXTURE2D, D3D11CreateDevice, ID3D11Device, ID3D11DeviceContext, ID3D11Texture2D, ID3D11VideoContext, ID3D11VideoDevice, ID3D11VideoProcessor, ID3D11VideoProcessorEnumerator, ID3D11VideoProcessorInputView, ID3D11VideoProcessorOutputView};
use windows::Win32::Graphics::Dxgi::{IDXGIAdapter, IDXGIDevice, IDXGIOutput, IDXGIOutput1, IDXGIOutputDuplication};
use windows::Win32::Graphics::Dxgi::Common::{DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_FORMAT_NV12, DXGI_SAMPLE_DESC};
use windows_core::Interface;
use crate::error::Error::Unknown;

use crate::recorder::parameters::VideoParams;
use crate::types::Result;

pub fn create_id3d11device(monitor: u32) -> Result<(ID3D11Device, ID3D11DeviceContext, IDXGIOutputDuplication)> {
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
    let device: ID3D11Device = device.ok_or(Unknown)?;
    let context: ID3D11DeviceContext = context.ok_or(Unknown)?;

    let adapter: IDXGIAdapter;
    let output: IDXGIOutput;
    let duplication: IDXGIOutputDuplication;

    let dxgi_device: IDXGIDevice = device.cast()?;
    unsafe {
        adapter = dxgi_device.GetAdapter()?;
        // Enumerate the first output (primary monitor).
        output = adapter.EnumOutputs(monitor)?;
    }
    let output1: IDXGIOutput1 = output.cast()?;
    unsafe {
        duplication = output1.DuplicateOutput(&device)?;
    }
    Ok((device, context, duplication))
}

pub fn get_hw_device_and_frame_cxt(device: &ID3D11Device) -> (*mut AVBufferRef, *mut AVBufferRef) {
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

pub unsafe fn convert_rgba_to_nv12(device: &ID3D11Device, context: &ID3D11DeviceContext, tex_rgba: &ID3D11Texture2D, in_width: u32, in_height: u32, out_width: u32, out_height: u32) -> Result<ID3D11Texture2D> {
    // 1) QI for ID3D11VideoDevice
    let video_dev: ID3D11VideoDevice = device.cast().unwrap();
    let video_ctx: ID3D11VideoContext = context.cast().unwrap();

    // 2) Describe & create the VideoProcessorEnumerator
    let vp_desc = D3D11_VIDEO_PROCESSOR_CONTENT_DESC {
        InputFrameFormat: D3D11_VIDEO_FRAME_FORMAT_PROGRESSIVE,
        InputFrameRate: Default::default(),
        InputWidth: in_width,
        InputHeight: in_height,
        OutputFrameRate: Default::default(),
        OutputWidth: out_width,
        OutputHeight: out_height,
        Usage: windows::Win32::Graphics::Direct3D11::D3D11_VIDEO_USAGE_OPTIMAL_SPEED,
    };
    let vp_enum: ID3D11VideoProcessorEnumerator = video_dev.CreateVideoProcessorEnumerator(&vp_desc).unwrap();


    //verify
    if vp_enum.CheckVideoProcessorFormat(DXGI_FORMAT_B8G8R8A8_UNORM).is_err() {
        panic!("DXGI_FORMAT_B8G8R8A8_UNORM not supported by ID3D11VideoProcessorEnumerator")
    }

    // 3) Create the VideoProcessor itself
    let vp: ID3D11VideoProcessor = video_dev.CreateVideoProcessor(&vp_enum, 0).unwrap();

    // 4) Make the NV12 output texture
    let nv12_desc = D3D11_TEXTURE2D_DESC {
        Width: out_width,
        Height: out_height,
        MipLevels: 1,
        ArraySize: 1,
        Format: DXGI_FORMAT_NV12,
        SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
        Usage: D3D11_USAGE_DEFAULT,
        BindFlags: (D3D11_BIND_SHADER_RESOURCE.0 | D3D11_BIND_RENDER_TARGET.0) as u32,
        CPUAccessFlags: 0,
        MiscFlags: 0,
    };
    let mut tex_nv12 = None;
    device.CreateTexture2D(&nv12_desc, None, Some(&mut tex_nv12)).unwrap();
    let tex_nv12 = tex_nv12.unwrap();

    // 5) Create processor‐input view for RGBA texture
    let in_view_desc = D3D11_VIDEO_PROCESSOR_INPUT_VIEW_DESC {
        FourCC: 0,
        ViewDimension: D3D11_VPIV_DIMENSION_TEXTURE2D,
        Anonymous: D3D11_VIDEO_PROCESSOR_INPUT_VIEW_DESC_0 {
            Texture2D: D3D11_TEX2D_VPIV {
                MipSlice: 0,
                ArraySlice: 0,
            },
        },
    };
    let mut input_view: Option<ID3D11VideoProcessorInputView> = None;
    video_dev.CreateVideoProcessorInputView(
        tex_rgba,
        &vp_enum,
        &in_view_desc,
        Some(&mut input_view),
    ).unwrap();                                                                // :contentReference[oaicite:7]{index=7}
    let input_view = input_view.unwrap();

    // 6) Create processor‐output view for NV12
    let out_view_desc = D3D11_VIDEO_PROCESSOR_OUTPUT_VIEW_DESC {
        ViewDimension: D3D11_VPOV_DIMENSION_TEXTURE2D,
        Anonymous: D3D11_VIDEO_PROCESSOR_OUTPUT_VIEW_DESC_0 {
            Texture2D: D3D11_TEX2D_VPOV {
                MipSlice: 0,
            },
        },
    };
    let mut output_view: Option<ID3D11VideoProcessorOutputView> = None;
    video_dev.CreateVideoProcessorOutputView(
        &tex_nv12,
        &vp_enum,
        &out_view_desc,
        Some(&mut output_view),
    ).unwrap();                                                                // :contentReference[oaicite:8]{index=8}
    let output_view = output_view.unwrap();

    // 7) Execute the GPU blit
    let _idk = ManuallyDrop::new(Some(input_view.clone()));
    let stream = D3D11_VIDEO_PROCESSOR_STREAM {
        Enable: TRUE,
        pInputSurface: _idk,
        ..Default::default()
    };
    video_ctx.VideoProcessorBlt(&vp, &output_view, 0, &[stream]).unwrap();            // :contentReference[oaicite:9]{index=9}

    // Return the new NV12 texture & processor
    Ok(tex_nv12)
}