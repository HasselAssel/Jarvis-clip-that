use anyhow::Context;
use anyhow::Result;
use windows::Win32::Graphics::Direct3D11::ID3D11Texture2D;
use windows::Win32::Graphics::Dxgi::{DXGI_ERROR_ACCESS_LOST, DXGI_ERROR_WAIT_TIMEOUT, DXGI_OUTDUPL_FRAME_INFO};
use windows::Win32::Graphics::Dxgi::IDXGIAdapter;
use windows::Win32::Graphics::Dxgi::IDXGIDevice;
use windows::Win32::Graphics::Dxgi::IDXGIOutput;
use windows::Win32::Graphics::Dxgi::IDXGIOutput1;
use windows::Win32::Graphics::Dxgi::IDXGIOutputDuplication;
use windows::Win32::Graphics::Dxgi::IDXGIResource;
use windows_core::Interface;

use crate::debug;
use crate::recorders::traits::Source;
use crate::recorders::windows::d3d11::env::EnvD3D11;

pub struct SourceD3d11 {
    duplication: Option<IDXGIOutputDuplication>,

    output1 : IDXGIOutput1,

    resource: Option<IDXGIResource>,
    frame_info: DXGI_OUTDUPL_FRAME_INFO,
}

impl SourceD3d11 {
    pub fn new(monitor: u32, env: &<Self  as Source>::Env<'_>) -> Result<Self> {
        let adapter: IDXGIAdapter;
        let output: IDXGIOutput;
        let duplication: Option<IDXGIOutputDuplication>;

        let dxgi_device: IDXGIDevice = env.device.cast()?;
        unsafe {
            adapter = dxgi_device.GetAdapter()?;
            output = adapter.EnumOutputs(monitor)?;
        }
        let output1: IDXGIOutput1 = output.cast()?;
        unsafe {
            duplication = output1.DuplicateOutput(&env.device).ok();
        }

        let resource = None;
        let frame_info = DXGI_OUTDUPL_FRAME_INFO::default();

        Ok(Self {
            duplication,
            output1,

            resource,
            frame_info,
        })
    }

    pub fn get_dims(&self) -> Option<(u32, u32)> {
        let desc = unsafe {
            self.duplication.as_ref()?.GetDesc()
        };

        let width = desc.ModeDesc.Width;
        let height = desc.ModeDesc.Height;

        Some((width, height))
    }
}

impl Source for SourceD3d11 {
    type Env<'e> = EnvD3D11;
    type Output<'o> = Option<AcquiredFrame<'o>>;

    fn next_frame(
        &mut self,
        env: &Self::Env<'_>
    ) -> Result<Self::Output<'_>> {
        // for some reason the first duplication.AcquireNextFrame call generates no IDXGIResource

        self.resource = None;

        let res = unsafe {
            self.duplication.as_ref().with_context(|| debug!("no duplication"))?.AcquireNextFrame(
                0,
                &mut self.frame_info,
                &mut self.resource,
            )
        };

        if let Err(err) = res {
            match err.code() {
                DXGI_ERROR_ACCESS_LOST => {
                    debug!("new duplication");
                    self.duplication = None;
                    self.duplication = Some(unsafe {
                        self.output1.DuplicateOutput(&env.device)?
                    });
                }
                DXGI_ERROR_WAIT_TIMEOUT => {}
                other => {
                    let _ = debug!(other);
                }
            }
            return Ok(None)
        }

        let mut acquired_frame = AcquiredFrame {
            _duplication: self.duplication.as_ref().with_context(|| debug!("no duplication"))?,
            texture: None,
            frame_info: &self.frame_info
        };

        let dxgi_resource = self.resource.as_ref().with_context(|| "no dxgi_resource!")?;

        if self.frame_info.AccumulatedFrames != 0 {
            acquired_frame.texture = Some(dxgi_resource.cast()?);
        }

        Ok(Some(acquired_frame))
    }
}


pub struct AcquiredFrame<'a> {
    texture: Option<ID3D11Texture2D>,
    pub frame_info: &'a DXGI_OUTDUPL_FRAME_INFO,
    _duplication: &'a IDXGIOutputDuplication,
}

impl AcquiredFrame<'_> {
    pub fn texture(&self) -> Option<&ID3D11Texture2D> {
        self.texture.as_ref()
    }
}

impl Drop for AcquiredFrame<'_> {
    fn drop(&mut self) {
        let _ = unsafe {
            self._duplication.ReleaseFrame()
        };
    }
}