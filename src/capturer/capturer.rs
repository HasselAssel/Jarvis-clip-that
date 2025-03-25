use windows::core::Interface;
use windows::Win32::Foundation::HMODULE;
use windows::Win32::Graphics::Direct3D11::{D3D11CreateDevice, ID3D11Device, ID3D11DeviceContext, D3D11_BIND_SHADER_RESOURCE, D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_SDK_VERSION, D3D11_TEXTURE2D_DESC, D3D11_USAGE_DEFAULT};
use windows::Win32::Graphics::Direct3D::{D3D_DRIVER_TYPE_HARDWARE, D3D_FEATURE_LEVEL_10_0, D3D_FEATURE_LEVEL_11_0};
use windows::Win32::Graphics::Dxgi::{IDXGIAdapter, IDXGIDevice, IDXGIOutput, IDXGIOutput1, IDXGIOutputDuplication};
use windows::Win32::Graphics::Dxgi::Common::{DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_SAMPLE_DESC};
use crate::capturer::ring_buffer::OptionalID3D11Texture2DRingBuffer;

struct Capturer<const LEN: usize> {
    duplication: IDXGIOutputDuplication,
    context: ID3D11DeviceContext,

    ring_buffer: OptionalID3D11Texture2DRingBuffer<LEN>
}

impl<const LEN: usize> Capturer<LEN> {
    pub unsafe fn new() -> Self {
        let mut device: Option<ID3D11Device> = None;
        let mut context: Option<ID3D11DeviceContext> = None;
        let _ = D3D11CreateDevice(
            None,
            D3D_DRIVER_TYPE_HARDWARE,
            HMODULE::default(),
            D3D11_CREATE_DEVICE_BGRA_SUPPORT,
            Some(&[D3D_FEATURE_LEVEL_11_0, D3D_FEATURE_LEVEL_10_0]),
            D3D11_SDK_VERSION,
            Some(&mut device),
            None,
            Some(&mut context),
        );
        let device = device.unwrap();
        let context = context.unwrap();
        // Get DXGI device and adapter from the D3D device.
        let dxgi_device: IDXGIDevice = device.cast()?;
        let adapter: IDXGIAdapter = dxgi_device.GetAdapter()?;
        // Enumerate the first output (primary monitor).
        let output: IDXGIOutput = adapter.EnumOutputs(0)?;
        let output1: IDXGIOutput1 = output.cast()?;

        // Create desktop duplication interface.
        let duplication = output1.DuplicateOutput(&device)?;

        // Retrieve desktop size.
        let mut out_desc = duplication.GetDesc();
        let width = out_desc.ModeDesc.Width;
        let height = out_desc.ModeDesc.Height;

        let tex_desc = D3D11_TEXTURE2D_DESC {
            Width: width,
            Height: height,
            MipLevels: 1,
            ArraySize: 1,
            Format: DXGI_FORMAT_B8G8R8A8_UNORM,
            SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
            Usage: D3D11_USAGE_DEFAULT,
            BindFlags: D3D11_BIND_SHADER_RESOURCE.0 as u32,
            CPUAccessFlags: 0,
            MiscFlags: 0,
        };
        let ring_buffer = OptionalID3D11Texture2DRingBuffer::<LEN>::new(&device, &tex_desc);

        Self {
            duplication,
            context,
            ring_buffer,
        }
    }

    pub fn start_capturing() {

    }
}