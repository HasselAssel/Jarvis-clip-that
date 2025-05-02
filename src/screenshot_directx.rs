use std::{thread::sleep, time::Duration};
use windows::{
    core::{Error, Result},
    Win32::{
        Graphics::{
            Direct3D11::*,
            Dxgi::*,
            Dxgi::Common::*,
        },
    },
};
use windows::Win32::Foundation::HMODULE;
use windows::Win32::Graphics::Direct3D::D3D_DRIVER_TYPE_HARDWARE;
use windows::core::Interface;
use image::{ImageBuffer, Rgba};

/// Captures 10 desktop screenshots using DirectX desktop duplication,
/// waits 10 seconds, then copies the first captured texture into a CPU-accessible staging texture.
/// Returns the staging texture.
///
/// **Disclaimer:** This is a simplified example. In real-world use you would
/// need to manage COM lifetimes, handle errors more gracefully, and possibly combine
/// the 10 screenshots into a single texture or return them all.
pub unsafe fn capture_desktop_screenshots() -> Result<(ID3D11DeviceContext, ID3D11Texture2D)> {
    // Create D3D11 device and immediate context
    let mut device: Option<ID3D11Device>  = None;
    let mut context: Option<ID3D11DeviceContext>  = None;
    D3D11CreateDevice(
        None,
        D3D_DRIVER_TYPE_HARDWARE,
        HMODULE::default(),
        D3D11_CREATE_DEVICE_BGRA_SUPPORT,
        None,
        D3D11_SDK_VERSION,
        Some(&mut device),
        None,
        Some(&mut context),
    )?;
    let device: ID3D11Device = device.unwrap();
    let context: ID3D11DeviceContext = context.unwrap();

    // Get DXGI device and adapter from the D3D device
    let dxgi_device: IDXGIDevice = device.cast()?;
    let adapter: IDXGIAdapter = dxgi_device.GetAdapter()?;
    // Enumerate the first output (primary monitor)
    let output: IDXGIOutput = adapter.EnumOutputs(0)?;
    let output1: IDXGIOutput1 = output.cast()?;

    // Create desktop duplication interface
    let mut duplication =  output1.DuplicateOutput(&device);
    let duplication: IDXGIOutputDuplication = duplication.unwrap();

    // Retrieve the output duplication description for the desktop size
    let mut out_desc = duplication.GetDesc();
    let width = out_desc.ModeDesc.Width;
    let height = out_desc.ModeDesc.Height;

    // Allocate a buffer to store up to 10 screenshots (for simplicity we just keep the texture pointers)
    let mut captured_textures = Vec::with_capacity(10);

    // Capture up to 10 frames
    for _ in 0..10 {
        let mut frame_info = DXGI_OUTDUPL_FRAME_INFO::default();
        let mut resource = None;
        // Try to acquire the next frame (wait up to 500ms for a new frame)
        let hr = duplication.AcquireNextFrame(500, &mut frame_info, &mut resource);
        if hr.is_err() {
            // In a real implementation you might handle timeouts differently
            continue;
        }
        if let Some(dxgi_resource) = resource {
            // Cast the DXGI resource to a D3D11 texture (the desktop image)
            let tex: ID3D11Texture2D = dxgi_resource.cast()?;
            captured_textures.push(tex);
        }
        duplication.ReleaseFrame()?;
    }

    // Wait for 10 seconds before fetching the GPU data to CPU
    sleep(Duration::from_secs(10));

    // For demonstration, we fetch the first captured texture.
    // To fetch data to CPU, we create a staging texture.
    let mut tex_desc = D3D11_TEXTURE2D_DESC {
        Width: width,
        Height: height,
        MipLevels: 1,
        ArraySize: 1,
        Format: DXGI_FORMAT_B8G8R8A8_UNORM, // Desktop duplication typically uses BGRA format
        SampleDesc: DXGI_SAMPLE_DESC {
            Count: 1,
            Quality: 0,
        },
        Usage: D3D11_USAGE_STAGING,
        BindFlags: 0,
        CPUAccessFlags: D3D11_CPU_ACCESS_READ.0 as u32,
        MiscFlags: 0,
    };

    let mut staging_texture: Option<ID3D11Texture2D> = None;
    device.CreateTexture2D(&tex_desc, None, Some(&mut staging_texture))?;
    let staging_texture = staging_texture.unwrap();

    // Ensure we have at least one screenshot captured
    if let Some(first_texture) = captured_textures.get(0) {
        // Copy the GPU texture into the staging texture so that it is CPU accessible
        context.CopyResource(&staging_texture, first_texture);
    } else {
        return Err(Error::from_win32());
    }

    // Return the staging texture (which now contains the image data in CPU-accessible memory)
    Ok((context, staging_texture))
}






/// Saves the provided staging texture (CPU-accessible) as a PNG file.
///
/// # Safety
/// This function uses unsafe Direct3D 11 mapping calls. Make sure the texture is
/// created as a staging texture with D3D11_USAGE_STAGING and CPU read access.
pub unsafe fn save_texture_to_png(
    context: &ID3D11DeviceContext,
    texture: &ID3D11Texture2D,
    file_path: &str,
) -> Result<()> {
    // Retrieve the texture description (width, height, etc.)
    let mut desc = D3D11_TEXTURE2D_DESC::default();
    texture.GetDesc(&mut desc);

    let width = desc.Width;
    let height = desc.Height;
    let pixel_size = 4; // For DXGI_FORMAT_B8G8R8A8_UNORM

    // Map the texture so we can access its data on the CPU.
    let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
    context.Map(texture, 0, D3D11_MAP_READ, 0, Some(&mut mapped))?;

    // Create a buffer to store the converted image data (in RGBA order).
    let mut pixels: Vec<u8> = Vec::with_capacity((width * height * pixel_size) as usize);
    // SAFETY: We immediately fill the vector below.
    pixels.set_len((width * height * pixel_size) as usize);

    // The mapped data might be padded, so copy row by row.
    for y in 0..height {
        // Calculate pointer for the start of this row.
        let src_ptr = (mapped.pData as *const u8).add((y * mapped.RowPitch) as usize);
        // Create a slice for the row (only width * pixel_size bytes)
        let row = std::slice::from_raw_parts(src_ptr, (width * pixel_size) as usize);

        // Process each pixel in the row.
        for x in 0..width {
            let i = (x * pixel_size) as usize;
            // Original order is BGRA; we want RGBA.
            let b = row[i];
            let g = row[i + 1];
            let r = row[i + 2];
            let a = row[i + 3];
            // Destination offset in our linear buffer.
            let dest_index = (y * width * pixel_size + x * pixel_size) as usize;
            pixels[dest_index] = r;
            pixels[dest_index + 1] = g;
            pixels[dest_index + 2] = b;
            pixels[dest_index + 3] = a;
        }
    }

    // Unmap the texture when done.
    context.Unmap(texture, 0);

    // Create an image buffer from our raw data.
    let img_buffer = ImageBuffer::<Rgba<u8>, _>::from_raw(width, height, pixels)
        .ok_or_else(|| windows::core::Error::new(
            windows::core::HRESULT(0),
            "Failed to create image buffer",
        ))?;

    // Save the image buffer as a PNG file.
    img_buffer.save(file_path)
        .map_err(|e| windows::core::Error::new(windows::core::HRESULT(0), e.to_string()))?;

    Ok(())
}