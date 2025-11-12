use std::mem::{ManuallyDrop, MaybeUninit};
use std::ptr::null_mut;
use ffmpeg_next::ffi::AVFrame;
use windows::core::Interface;
use windows::Win32::Foundation::{HMODULE, TRUE};
use windows::Win32::Graphics::Direct3D11::{D3D11_BIND_RENDER_TARGET, D3D11_BIND_SHADER_RESOURCE, D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_SDK_VERSION, D3D11_TEX2D_VPIV, D3D11_TEX2D_VPOV, D3D11_TEXTURE2D_DESC, D3D11_USAGE_DEFAULT, D3D11_VIDEO_FRAME_FORMAT_PROGRESSIVE, D3D11_VIDEO_PROCESSOR_CONTENT_DESC, D3D11_VIDEO_PROCESSOR_INPUT_VIEW_DESC, D3D11_VIDEO_PROCESSOR_INPUT_VIEW_DESC_0, D3D11_VIDEO_PROCESSOR_OUTPUT_VIEW_DESC, D3D11_VIDEO_PROCESSOR_OUTPUT_VIEW_DESC_0, D3D11_VIDEO_PROCESSOR_STREAM, D3D11_VPIV_DIMENSION_TEXTURE2D, D3D11_VPOV_DIMENSION_TEXTURE2D, D3D11CreateDevice, ID3D11Device, ID3D11DeviceContext, ID3D11Texture2D, ID3D11VideoContext, ID3D11VideoDevice, ID3D11VideoProcessor, ID3D11VideoProcessorEnumerator, ID3D11VideoProcessorInputView, ID3D11VideoProcessorOutputView};
use windows::Win32::Graphics::Direct3D::{D3D_DRIVER_TYPE_HARDWARE, D3D_FEATURE_LEVEL_10_0, D3D_FEATURE_LEVEL_11_0};
use windows::Win32::Graphics::Dxgi::{DXGI_OUTDUPL_DESC, DXGI_OUTDUPL_FRAME_INFO, IDXGIAdapter, IDXGIDevice, IDXGIOutput, IDXGIOutput1, IDXGIOutputDuplication, IDXGIResource};
use windows::Win32::Graphics::Dxgi::Common::{DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_FORMAT_NV12, DXGI_FORMAT_R8G8B8A8_UNORM, DXGI_SAMPLE_DESC};
use crate::error::Error::Unknown;
use crate::recorders::video::sources::d3d111::traits::D3d11EncoderHwContext;
use crate::recorders::video::sources::traits::VideoSource;
use crate::wrappers::MaybeSafeFFIPtrWrapper;
use crate::types::Result;

pub struct VideoSourceD3d11<E: D3d11EncoderHwContext> {
    pub device: ID3D11Device,
    context: ID3D11DeviceContext,
    duplication: IDXGIOutputDuplication,

    resource: Option<IDXGIResource>,
    hr: windows::core::Result<()>,
    frame_info: DXGI_OUTDUPL_FRAME_INFO,
    device_tex: ID3D11Texture2D,
    nv12_tex: ID3D11Texture2D,

    in_desc: DXGI_OUTDUPL_DESC,

    pub encoder_hw_ctx: E,
}

impl<E: D3d11EncoderHwContext> VideoSourceD3d11<E> {
    pub fn new(monitor: u32, encoder_hw_ctx: E) -> Self {
        let (device, context, duplication) = create_id3d11(monitor).unwrap();

        let resource = None;
        let hr = unsafe { MaybeUninit::zeroed().assume_init() };
        let frame_info = DXGI_OUTDUPL_FRAME_INFO::default();

        let dummy_desc = D3D11_TEXTURE2D_DESC {
            Width: 1,
            Height: 1,
            MipLevels: 1,
            ArraySize: 1,
            Format: DXGI_FORMAT_R8G8B8A8_UNORM,
            SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
            Usage: D3D11_USAGE_DEFAULT,
            BindFlags: D3D11_BIND_SHADER_RESOURCE.0 as u32,
            CPUAccessFlags: 0,
            MiscFlags: 0,
        };

        let mut device_tex = None;
        let mut nv12_tex = None;

        unsafe {
            device.CreateTexture2D(&dummy_desc, Some(null_mut()), Some(&mut device_tex)).unwrap();
            device.CreateTexture2D(&dummy_desc, Some(null_mut()), Some(&mut nv12_tex)).unwrap();
        }

        let device_tex = device_tex.unwrap();
        let nv12_tex = nv12_tex.unwrap();

        let in_desc = unsafe { duplication.GetDesc() };

        Self {
            device,
            context,
            duplication,

            resource,
            hr,
            frame_info,
            device_tex,
            nv12_tex,

            in_desc,

            encoder_hw_ctx,
        }
    }
}

impl<E: D3d11EncoderHwContext> VideoSource for VideoSourceD3d11<E> {
    fn init(&mut self) {
    }

    fn get_frame(&mut self, av_frame: &MaybeSafeFFIPtrWrapper<AVFrame>, out_width: u32, out_height: u32) -> std::result::Result<(), String> {
        // TODO: Fix First Frame always being Green (for some reason the first duplication.AcquireNextFrame call generates no IDXGIResource)

        self.resource = None;

        unsafe { self.hr = self.duplication.AcquireNextFrame(0, &mut self.frame_info, &mut self.resource); }

        if self.hr.is_err() {
            return Err(format!("IDXGIOutputDuplication::AcquireNextFrame returned Error value: {:?}", self.hr));
        }

        if let Some(dxgi_resource) = &self.resource {
            if self.frame_info.AccumulatedFrames != 0 {
                self.device_tex = dxgi_resource.cast().unwrap();

                self.nv12_tex = unsafe {
                    convert_rgba_to_nv12(&self.device, &self.context, &self.device_tex, self.in_desc.ModeDesc.Width, self.in_desc.ModeDesc.Height, out_width, out_height).unwrap()
                };

                self.encoder_hw_ctx.prepare_frame(av_frame, &self.nv12_tex).unwrap();
            }
        }

        unsafe { self.duplication.ReleaseFrame().unwrap(); }

        Ok(())
    }
}


fn create_id3d11(monitor: u32) -> Result<(ID3D11Device, ID3D11DeviceContext, IDXGIOutputDuplication)> {
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
        output = adapter.EnumOutputs(monitor)?;
    }
    let output1: IDXGIOutput1 = output.cast()?;
    unsafe {
        duplication = output1.DuplicateOutput(&device)?;
    }
    Ok((device, context, duplication))
}

unsafe fn convert_rgba_to_nv12(device: &ID3D11Device, context: &ID3D11DeviceContext, tex_rgba: &ID3D11Texture2D, in_width: u32, in_height: u32, out_width: u32, out_height: u32) -> Result<ID3D11Texture2D> {
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
    ).unwrap();
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
    ).unwrap();
    let output_view = output_view.unwrap();

    // 7) Execute the GPU blit
    let _idk = ManuallyDrop::new(Some(input_view.clone()));
    let stream = D3D11_VIDEO_PROCESSOR_STREAM {
        Enable: TRUE,
        pInputSurface: _idk,
        ..Default::default()
    };
    video_ctx.VideoProcessorBlt(&vp, &output_view, 0, &[stream]).unwrap();

    // Return the new NV12 texture & processor
    Ok(tex_nv12)
}