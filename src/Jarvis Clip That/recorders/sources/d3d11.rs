use anyhow::anyhow;
use anyhow::Result;
use windows::Win32::Graphics::Direct3D11::ID3D11Texture2D;
use windows::Win32::Graphics::Dxgi::DXGI_OUTDUPL_FRAME_INFO;
use windows::Win32::Graphics::Dxgi::IDXGIAdapter;
use windows::Win32::Graphics::Dxgi::IDXGIDevice;
use windows::Win32::Graphics::Dxgi::IDXGIOutput;
use windows::Win32::Graphics::Dxgi::IDXGIOutput1;
use windows::Win32::Graphics::Dxgi::IDXGIOutputDuplication;
use windows::Win32::Graphics::Dxgi::IDXGIResource;
use windows_core::Interface;

use crate::recorders::env::EnvD3D11;
use crate::recorders::traits::Source;

pub struct SourceD3d11 {
    duplication: IDXGIOutputDuplication,

    resource: Option<IDXGIResource>,
    frame_info: DXGI_OUTDUPL_FRAME_INFO,
}

impl SourceD3d11 {
    pub fn new(monitor: u32, env: <Self  as Source>::Env<'_>) -> Result<Self> {
        let adapter: IDXGIAdapter;
        let output: IDXGIOutput;
        let duplication: IDXGIOutputDuplication;

        let dxgi_device: IDXGIDevice = env.device.cast()?;
        unsafe {
            adapter = dxgi_device.GetAdapter()?;
            output = adapter.EnumOutputs(monitor)?;
        }
        let output1: IDXGIOutput1 = output.cast()?;
        unsafe {
            duplication = output1.DuplicateOutput(&env.device)?;
        }

        let resource = None;
        let frame_info = DXGI_OUTDUPL_FRAME_INFO::default();

        Ok(Self {
            duplication,

            resource,
            frame_info,
        })
    }
}

impl Source for SourceD3d11 {
    type Env<'e> = &'e EnvD3D11;
    type Output<'o> = AcquiredFrame<'o>;

    fn next_frame(&mut self, _: Self::Env<'_>) -> Result<Self::Output<'_>> {
        // TODO: Fix First Frame always being Green (for some reason the first duplication.AcquireNextFrame call generates no IDXGIResource)

        self.resource = None;

        unsafe {
            self.duplication.AcquireNextFrame(
                0,
                &mut self.frame_info,
                &mut self.resource,
            )?;
        }

        let mut acquired_frame = AcquiredFrame {
            duplication: &self.duplication,
            texture: None,
        };

        let dxgi_resource = self.resource.as_ref().ok_or_else(|| anyhow!("no dxgi_resource!"))?;

        if self.frame_info.AccumulatedFrames != 0 {
            acquired_frame.texture = Some(dxgi_resource.cast()?);
        }

        Ok(acquired_frame)
    }
}


pub struct AcquiredFrame<'a> {
    duplication: &'a IDXGIOutputDuplication,
    texture: Option<ID3D11Texture2D>,
}

impl AcquiredFrame<'_> {
    pub fn texture(&self) -> Option<&ID3D11Texture2D> {
        self.texture.as_ref()
    }
}

impl Drop for AcquiredFrame<'_> {
    fn drop(&mut self) {
        let _ = unsafe {
            self.duplication.ReleaseFrame()
        };
    }
}