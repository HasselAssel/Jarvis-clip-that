use std::{array, ptr, thread, thread::sleep, time::Duration};
use std::cmp::max;
use std::collections::VecDeque;
use std::time::Instant;
use windows::{
    core::{Error, HSTRING, Result},
    Win32::{
        Graphics::{
            Direct3D11::*,
            Dxgi::*,
            Dxgi::Common::*,
        },
    },
};
use image::{ImageBuffer, Rgba};
use windows::core::Interface;
use windows::Win32::Foundation::HMODULE;
use windows::Win32::Graphics::Direct3D::{D3D_DRIVER_TYPE_HARDWARE, D3D_FEATURE_LEVEL_10_0, D3D_FEATURE_LEVEL_11_0};

struct OptionalID3D11Texture2D {
    tex: ID3D11Texture2D,
    is_valid: bool,
}

struct OptionalID3D11Texture2DRingBuffer<const N: usize> {
    data: [OptionalID3D11Texture2D; N],
    index: usize,
}

impl<const N: usize> OptionalID3D11Texture2DRingBuffer<N> {
    unsafe fn new(device: &ID3D11Device, tex_desc: &D3D11_TEXTURE2D_DESC) -> Self {
        Self {
            data: array::from_fn(|_|
                            {let mut dest_texture = None;
                            device.CreateTexture2D(tex_desc, None, Some(&mut dest_texture)).unwrap();
                                OptionalID3D11Texture2D{
                                    tex: dest_texture.unwrap(),
                                    is_valid: false}    }),
            index: 0
        }
    }
}


pub unsafe fn capture_desktop_screenshots() -> Result<(ID3D11DeviceContext, ID3D11Texture2D)> {
    sleep(Duration::from_millis(2000));

    // Create D3D11 device and immediate context.
    let mut device: Option<ID3D11Device> = None;
    let mut context: Option<ID3D11DeviceContext> = None;
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


    let mut resource: Option<IDXGIResource>;
    let mut hr: Result<()>;
    let mut frame_info = DXGI_OUTDUPL_FRAME_INFO::default();
    let mut tex: ID3D11Texture2D;
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

    const LEN: usize = 500;
    let mut captured_textures = OptionalID3D11Texture2DRingBuffer::<LEN>::new(&device, &tex_desc);

    let fps: u32 = 24;
    let mspf: u32 = 1000 / fps;
    let frame_duration = Duration::from_millis(mspf);
    println!("{}", mspf);

    
    let mut elapsed: Duration;
    let mut expected_elapsed: Duration;
    let start_time = Instant::now();
    for n in 0..LEN {
        elapsed = start_time.elapsed();
        expected_elapsed = frame_time.saturating_mul(n);
        if expected_elapsed > elapsed {
            sleep(expected_elapsed - elapsed);
        }

        resource = None; // maybe removeable?
        hr = duplication.AcquireNextFrame(mspf, &mut frame_info, &mut resource);
        if hr.is_err() {
            captured_textures.data[captured_textures.index].is_valid = false;
            captured_textures.index += 1;
            println!("why???");
            continue;
        }
        if let Some(dxgi_resource) = resource {
            if frame_info.AccumulatedFrames == 0 {
                duplication.ReleaseFrame()?;
                captured_textures.data[captured_textures.index].is_valid = false;
                captured_textures.index += 1;
                continue;
            }
            tex = dxgi_resource.cast()?;

            // Copy the acquired frame to the destination texture
            context.CopyResource(&captured_textures.data[captured_textures.index].tex, &tex);
            captured_textures.data[captured_textures.index].is_valid = true;
            captured_textures.index += 1;

        }
        duplication.ReleaseFrame()?;
    }
    println!("Time elapsed: {}", start_time.elapsed().as_millis()); println!("Delta time: {}", delta_time );
    println!("Frames captured: {}", captured_textures.index); println!("FPS: {}", (captured_textures.index as u128 * 1000) / start_time.elapsed().as_millis() );


    // Create a staging texture (CPU-accessible) for one captured image.
    let tex_desc = D3D11_TEXTURE2D_DESC {
        Width: width,
        Height: height,
        MipLevels: 1,
        ArraySize: 1,
        Format: DXGI_FORMAT_B8G8R8A8_UNORM,
        SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
        Usage: D3D11_USAGE_STAGING,
        BindFlags: 0,
        CPUAccessFlags: D3D11_CPU_ACCESS_READ.0 as u32,
        MiscFlags: 0,
    };

    let mut staging_texture: Option<ID3D11Texture2D> = None;
    device.CreateTexture2D(&tex_desc, None, Some(&mut staging_texture))?;
    let staging_texture = staging_texture.unwrap();

    // Copy the first captured frame into the staging texture.
    if let one_texture = &captured_textures.data[0] {
        context.CopyResource(&staging_texture, &one_texture.tex);
    } else {
        return Err(Error::new(windows::core::HRESULT(0), "No screenshot was captured"));
    }

    Ok((context, staging_texture))
}


pub unsafe fn save_texture_to_png(
    context: &ID3D11DeviceContext,
    texture: &ID3D11Texture2D,
    file_path: &str,
) -> Result<()> {
    // Retrieve texture description.
    let mut desc = D3D11_TEXTURE2D_DESC::default();
    texture.GetDesc(&mut desc);

    let width = desc.Width;
    let height = desc.Height;
    let pixel_size = 4; // For DXGI_FORMAT_B8G8R8A8_UNORM.

    // Map the texture to access its data on the CPU.
    let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
    let hr = context.Map(texture, 0, D3D11_MAP_READ, 0, Some(&mut mapped));
    if hr.is_err() {
        println!("{:?}", hr);
    }

    // Create a buffer to store the converted image data (in RGBA order).
    let mut pixels: Vec<u8> = Vec::with_capacity((width * height * pixel_size) as usize);
    pixels.set_len((width * height * pixel_size) as usize);

    // The mapped data might be padded, so copy row by row.
    for y in 0..height {
        let src_ptr = (mapped.pData as *const u8).add((y * mapped.RowPitch) as usize);
        let row = std::slice::from_raw_parts(src_ptr, (width * pixel_size) as usize);
        for x in 0..width {
            let i = (x * pixel_size) as usize;
            let b = row[i];
            let g = row[i + 1];
            let r = row[i + 2];
            let a = row[i + 3];
            let dest_index = (y * width * pixel_size + x * pixel_size) as usize;
            pixels[dest_index] = r;
            pixels[dest_index + 1] = g;
            pixels[dest_index + 2] = b;
            pixels[dest_index + 3] = a;
        }
    }

    // Unmap the texture.
    context.Unmap(texture, 0);

    // Create an image buffer from the raw data and save as PNG.
    let img_buffer = ImageBuffer::<Rgba<u8>, _>::from_raw(width, height, pixels)
        .ok_or_else(|| {
            Error::new(windows::core::HRESULT(0), "Failed to create image buffer")
        })?;
    img_buffer.save(file_path)
        .map_err(|e| Error::new(windows::core::HRESULT(0), e.to_string()))?;
    Ok(())
}
