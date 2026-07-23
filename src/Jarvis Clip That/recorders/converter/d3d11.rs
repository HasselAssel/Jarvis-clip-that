use std::mem::ManuallyDrop;

use anyhow::{anyhow, bail, Context, Result};
use windows::Win32::Foundation::{RECT, TRUE};
use windows::Win32::Graphics::Direct3D11::{D3D11_BIND_RENDER_TARGET, D3D11_TEX2D_VPIV, D3D11_TEX2D_VPOV, D3D11_TEXTURE2D_DESC, D3D11_USAGE_DEFAULT, D3D11_VIDEO_FRAME_FORMAT_PROGRESSIVE, D3D11_VIDEO_PROCESSOR_CONTENT_DESC, D3D11_VIDEO_PROCESSOR_FORMAT_SUPPORT_INPUT, D3D11_VIDEO_PROCESSOR_FORMAT_SUPPORT_OUTPUT, D3D11_VIDEO_PROCESSOR_INPUT_VIEW_DESC, D3D11_VIDEO_PROCESSOR_INPUT_VIEW_DESC_0, D3D11_VIDEO_PROCESSOR_OUTPUT_VIEW_DESC, D3D11_VIDEO_PROCESSOR_OUTPUT_VIEW_DESC_0, D3D11_VIDEO_PROCESSOR_STREAM, D3D11_VPIV_DIMENSION_TEXTURE2D, D3D11_VPOV_DIMENSION_TEXTURE2D, ID3D11Texture2D, ID3D11VideoContext, ID3D11VideoDevice, ID3D11VideoProcessor, ID3D11VideoProcessorEnumerator, ID3D11VideoProcessorInputView, ID3D11VideoProcessorOutputView};
use windows::Win32::Graphics::Dxgi::Common::{DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_FORMAT_NV12, DXGI_RATIONAL, DXGI_SAMPLE_DESC};
use windows_core::Interface;

use crate::recorders::env::EnvD3D11;
use crate::recorders::sources::d3d11::AcquiredFrame;
use crate::recorders::traits::Converter;

pub struct BgraToNv12Converter {
    video_device: ID3D11VideoDevice,
    video_context: ID3D11VideoContext,
    enumerator: ID3D11VideoProcessorEnumerator,
    processor: ID3D11VideoProcessor,
    nv12_desc: D3D11_TEXTURE2D_DESC,

    input_width: u32,
    input_height: u32,
}


impl BgraToNv12Converter {
    pub fn new((input_width, input_height) : (u32, u32), (output_width, output_height) : (u32, u32), env: <Self as Converter>::Env<'_>) -> Result<Self> {
        if input_width == 0 || input_height == 0 {
            bail!("input dimensions must be nonzero, got {}x{}", input_width, input_height);
        }

        if output_width == 0 || output_height == 0 {
            bail!("output dimensions must be nonzero, got {}x{}", output_width, output_height);
        }

        if output_width % 2 != 0 || output_height % 2 != 0 {
            bail!("NV12 output dimensions must be even, got {}x{}", output_width, output_height);
        }

        let video_device: ID3D11VideoDevice = env.device.cast()?;
        let video_context: ID3D11VideoContext = env.context.cast()?;

        let vp_desc = D3D11_VIDEO_PROCESSOR_CONTENT_DESC {
            InputFrameFormat: D3D11_VIDEO_FRAME_FORMAT_PROGRESSIVE,
            InputFrameRate: DXGI_RATIONAL {Numerator: 1, Denominator: 1},
            InputWidth: input_width,
            InputHeight: input_height,
            OutputFrameRate: DXGI_RATIONAL {Numerator: 1, Denominator: 1},
            OutputWidth: output_width,
            OutputHeight: output_height,
            Usage: windows::Win32::Graphics::Direct3D11::D3D11_VIDEO_USAGE_OPTIMAL_SPEED,
        };

        let enumerator: ID3D11VideoProcessorEnumerator = unsafe {
            video_device.CreateVideoProcessorEnumerator(&vp_desc)?
        };

        let bgra_support = unsafe {
            enumerator
                .CheckVideoProcessorFormat(DXGI_FORMAT_B8G8R8A8_UNORM)
                .context("failed to query BGRA video-processor support")?
        };
        if bgra_support & D3D11_VIDEO_PROCESSOR_FORMAT_SUPPORT_INPUT.0 as u32 == 0 {
            bail!("BGRA is not supported as video-processor input");
        }

        let nv12_support = unsafe {
            enumerator
                .CheckVideoProcessorFormat(DXGI_FORMAT_NV12)
                .context("failed to query NV12 video-processor support")?
        };
        if nv12_support & D3D11_VIDEO_PROCESSOR_FORMAT_SUPPORT_OUTPUT.0 as u32 == 0 {
            bail!("NV12 is not supported as video-processor output");
        }

        let processor: ID3D11VideoProcessor = unsafe {
            video_device.CreateVideoProcessor(&enumerator, 0)?
        };

        let src_rect = RECT {
            left: 0,
            top: 0,
            right: i32::try_from(input_width).with_context(|| format!("input_width exceeds i32::MAX: {input_width}"))?,
            bottom: i32::try_from(input_height).with_context(|| format!("input_height exceeds i32::MAX: {input_height}"))?,
        };

        let dst_rect = RECT {
            left: 0,
            top: 0,
            right: i32::try_from(output_width).with_context(|| format!("output_width exceeds i32::MAX: {output_width}"))?,
            bottom: i32::try_from(output_height).with_context(|| format!("output_height exceeds i32::MAX: {output_height}"))?,
        };

        unsafe {
            video_context.VideoProcessorSetStreamSourceRect(
                &processor,
                0,
                true,
                Some(&src_rect),
            );

            video_context.VideoProcessorSetStreamDestRect(
                &processor,
                0,
                true,
                Some(&dst_rect),
            );

            video_context.VideoProcessorSetOutputTargetRect(
                &processor,
                true,
                Some(&dst_rect),
            );
        }

        let nv12_desc = D3D11_TEXTURE2D_DESC {
            Width: output_width,
            Height: output_height,
            MipLevels: 1,
            ArraySize: 1,
            Format: DXGI_FORMAT_NV12,
            SampleDesc: DXGI_SAMPLE_DESC {
                Count: 1,
                Quality: 0,
            },
            Usage: D3D11_USAGE_DEFAULT,
            BindFlags: D3D11_BIND_RENDER_TARGET.0 as u32,
            CPUAccessFlags: 0,
            MiscFlags: 0,
        };

        Ok(Self {
            video_device,
            video_context,
            enumerator,
            processor,
            nv12_desc,

            input_width,
            input_height,
        })
    }
}

impl Converter for BgraToNv12Converter {
    type Env<'e> = &'e EnvD3D11;
    type Input<'i> = AcquiredFrame<'i>;
    type Output<'o> = ID3D11Texture2D;

    fn convert(
        &mut self,
        input: Self::Input<'_>,
        env: Self::Env<'_>
    ) -> Result<Self::Output<'_>> {
        let texture_bgra = input.texture().ok_or_else(|| anyhow!("No Texture provided"))?;

        let mut input_desc = D3D11_TEXTURE2D_DESC::default();

        unsafe {
            texture_bgra.GetDesc(&mut input_desc);
        }

        if input_desc.Format != DXGI_FORMAT_B8G8R8A8_UNORM {
            bail!("expected DXGI_FORMAT_B8G8R8A8_UNORM, got {:?}", input_desc.Format);
        }

        if input_desc.Width != self.input_width || input_desc.Height != self.input_height {
            bail!(
                "input resolution changed: converter expects {}x{}, texture is {}x{}",
                self.input_width, self.input_height, input_desc.Width, input_desc.Height
            );
        }

        if input_desc.ArraySize != 1 {
            bail!("expected a single input texture, got array size {}", input_desc.ArraySize);
        }

        if input_desc.SampleDesc.Count != 1 {
            bail!("multisampled input textures are unsupported; sample count is {}", input_desc.SampleDesc.Count);
        }

        let mut tex_nv12 = None;
        unsafe {
            env.device.CreateTexture2D(&self.nv12_desc, None, Some(&mut tex_nv12))?
        };
        let tex_nv12 = tex_nv12.ok_or_else(|| anyhow!("Can't create Texture"))?;

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
        unsafe {
            self.video_device.CreateVideoProcessorInputView(
                texture_bgra,
                &self.enumerator,
                &in_view_desc,
                Some(&mut input_view),
            )?;
        }
        let input_view = input_view.ok_or_else(|| anyhow!("idk bro"))?;

        let out_view_desc = D3D11_VIDEO_PROCESSOR_OUTPUT_VIEW_DESC {
            ViewDimension: D3D11_VPOV_DIMENSION_TEXTURE2D,
            Anonymous: D3D11_VIDEO_PROCESSOR_OUTPUT_VIEW_DESC_0 {
                Texture2D: D3D11_TEX2D_VPOV {
                    MipSlice: 0,
                },
            },
        };
        let mut output_view: Option<ID3D11VideoProcessorOutputView> = None;
        unsafe {
            self.video_device.CreateVideoProcessorOutputView(
                &tex_nv12,
                &self.enumerator,
                &out_view_desc,
                Some(&mut output_view),
            )?;
        }
        let output_view = output_view.ok_or_else(|| anyhow!("No output view idk???"))?;

        let mut streams = [D3D11_VIDEO_PROCESSOR_STREAM {
            Enable: TRUE,
            OutputIndex: 0,
            InputFrameOrField: 0,
            PastFrames: 0,
            FutureFrames: 0,
            pInputSurface: ManuallyDrop::new(Some(input_view)),
            ..Default::default()
        }];
        let blit_result = unsafe {
            self.video_context.VideoProcessorBlt(
                &self.processor,
                &output_view,
                0,
                &streams,
            )
        };

        unsafe {
            ManuallyDrop::drop(&mut streams[0].pInputSurface);
        }

        blit_result.context("BGRA-to-NV12 conversion failed")?;

        Ok(tex_nv12)
    }
}