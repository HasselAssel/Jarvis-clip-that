use std::ptr;
use windows::{
    core::*,
    Win32::Foundation::*,
    Win32::Graphics::Direct3D11::*,
    Win32::Graphics::Dxgi::*,
    Win32::Graphics::Dxgi::Common::*,
};
use windows::Win32::Graphics::Direct3D::D3D_DRIVER_TYPE_HARDWARE;

/// Captures the current desktop frame by duplicating the output,
/// copying it into a CPU-readable staging texture, and returning the pixel data.
fn capture_frame() -> Result<Vec<u8>> {
    unsafe {
        // 1. Create a D3D11 device and context.
        let mut device: Option<ID3D11Device> = None;
        let mut context = None;
        D3D11CreateDevice(
            None,
            D3D_DRIVER_TYPE_HARDWARE,
            HMODULE::default(),
            D3D11_CREATE_DEVICE_BGRA_SUPPORT,
            None,
            D3D11_SDK_VERSION,
            Some(&mut device),
            None,
            None,
        ).ok();
        let device = device.unwrap();
        let context = context.unwrap();

        // 2. Get the DXGI device from the D3D11 device.
        let dxgi_device: IDXGIDevice = device.cast()?;

        // 3. Get the adapter.
        let adapter = dxgi_device.GetAdapter()?;

        // 4. Get the first output (primary monitor).
        let mut output = adapter.EnumOutputs(0).ok();
        let output = output.unwrap();

        // 5. Query for IDXGIOutput1 (required for duplication).
        let output1: IDXGIOutput1 = output.cast()?;

        // 6. Duplicate the output (desktop duplication).
        let duplication = output1.DuplicateOutput(&device)?;

        // 7. Acquire the next frame.
        let mut frame_info = DXGI_OUTDUPL_FRAME_INFO::default();
        let mut resource = None;
        duplication.AcquireNextFrame(500, &mut frame_info, &mut resource).ok()?;
        let frame_resource = resource.unwrap();

        // 8. Obtain the captured frame as an ID3D11Texture2D.
        let frame_texture: ID3D11Texture2D = frame_resource.cast()?;

        // 9. Get the texture description.
        let mut desc = D3D11_TEXTURE2D_DESC::default();
        frame_texture.GetDesc(&mut desc);

        // 10. Create a staging texture (GPU buffer) for CPU read access.
        let mut staging_desc = desc;
        staging_desc.Usage = D3D11_USAGE_STAGING;
        staging_desc.BindFlags = 0;
        staging_desc.CPUAccessFlags = D3D11_CPU_ACCESS_READ;
        staging_desc.MiscFlags = 0;
        let staging_texture = device.CreateTexture2D(&staging_desc, ptr::null())?;

        // 11. Copy the captured frame to the staging texture.
        context.CopyResource(&staging_texture, &frame_texture);

        // 12. Map the staging texture to read the pixel data.
        let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
        context.Map(&staging_texture, 0, D3D11_MAP_READ, 0, &mut mapped).ok()?;

        // Calculate the size in bytes (using RowPitch * height).
        let row_pitch = mapped.RowPitch as usize;
        let height = desc.Height as usize;
        let data_size = row_pitch * height;
        let mut data = vec![0u8; data_size];

        // Copy the data from the mapped memory.
        ptr::copy_nonoverlapping(mapped.pData as *const u8, data.as_mut_ptr(), data_size);

        // Unmap the staging texture.
        context.Unmap(&staging_texture, 0);

        // 13. Release the frame.
        duplication.ReleaseFrame().ok()?;

        Ok(data)
    }
}

fn main() -> Result<()> {
    let frame_data = capture_frame()?;
    println!("Captured frame with {} bytes", frame_data.len());
    Ok(())
}
