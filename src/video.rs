use std::{fs::File, io::BufWriter, mem, ptr};
use winapi::{
    shared::{windef::HBITMAP, minwindef::DWORD},
    um::{
        wingdi::{
            BITMAP, BITMAPINFO, BITMAPINFOHEADER, GetDIBits, GetObjectW, CreateCompatibleDC, DeleteDC,
            BI_RGB, DIB_RGB_COLORS, SelectObject
        },
        winuser::{GetDC, ReleaseDC},
    },
    ctypes::c_void
};
use image::{ImageBuffer, RgbaImage, codecs::png::PngEncoder, ImageEncoder};

use rgb2yuv420::convert_rgb_to_yuv420p;

use openh264::encoder::{BitRate, Encoder, EncoderConfig, FrameRate, IntraFramePeriod, QpRange, RateControlMode};
use openh264::formats::{YUVBuffer, BgrSliceU8};
use openh264::OpenH264API;


use crate::{TimeTracker};

pub fn hbitmap_to_png(hbitmap: HBITMAP, output_path: &str) -> Result<(), String> {
    let mut tt = TimeTracker::new();
    unsafe {
        // Get bitmap dimensions and format
        let mut bitmap: BITMAP = mem::zeroed();
        if GetObjectW(
            hbitmap as *mut _,
            size_of::<BITMAP>() as i32,
            &mut bitmap as *mut BITMAP as *mut c_void // Explicit cast chain
        ) == 0
        {
            return Err("Failed to get bitmap info".into());
        }

        tt.time_since_last_marker(); // DEBUG

        let width = bitmap.bmWidth;
        let height = bitmap.bmHeight.abs(); // Handle negative height (top-down)
        let bpp = bitmap.bmBitsPixel;

        // Setup BITMAPINFO structure
        let mut bmi: BITMAPINFOHEADER = mem::zeroed();
        bmi.biSize = mem::size_of::<BITMAPINFOHEADER>() as DWORD;
        bmi.biWidth = width;
        bmi.biHeight = -height; // Negative for top-down DIB
        bmi.biPlanes = 1;
        bmi.biBitCount = 32;
        bmi.biCompression = BI_RGB;

        tt.time_since_last_marker(); // DEBUG

        let mut bitmap_info = BITMAPINFO {
            bmiHeader: bmi,
            bmiColors: [mem::zeroed()], // Initialize with zeroed RGBQUAD
        };

        // Create buffer for pixel data
        let buffer_size = (width * height * 4) as usize;
        let mut buffer: Vec<u8> = vec![0; buffer_size];

        tt.time_since_last_marker(); // DEBUG

        // Get device context
        let hdc = GetDC(ptr::null_mut());

        // Retrieve pixel data
        if GetDIBits(
            hdc,
            hbitmap,
            0,
            height as u32,
            buffer.as_mut_ptr() as *mut c_void,
            &mut bitmap_info as *mut _ as *mut _,
            DIB_RGB_COLORS,
        ) == 0
        {
            return Err("Failed to get bitmap bits".into());
        }

        tt.time_since_last_marker(); // DEBUG

        // Convert BGRA to RGBA and handle alpha channel
        for chunk in buffer.chunks_exact_mut(4) {
            // Swap red and blue channels (BGRA -> RGBA)
            chunk.swap(0, 2);

            // Set alpha to 255 if source was 24bpp
            if bpp == 24 {
                chunk[3] = 255;
            }
        }

        tt.time_since_last_marker(); // DEBUG

        // Create image and save as PNG
        let image: RgbaImage = ImageBuffer::from_raw(
            width as u32,
            height as u32,
            buffer,
        ).ok_or("Failed to create image buffer")?;

        tt.time_since_last_marker(); // DEBUG

        /*image.save(output_path)
            .map_err(|e| format!("Failed to save PNG: {}", e))?;*/

        let mut file = BufWriter::new(File::create(output_path).map_err(|e| format!("Failed to create file: {}", e))?);
        let encoder = PngEncoder::new_with_quality(
            &mut file,
            image::codecs::png::CompressionType::Fast,
            image::codecs::png::FilterType::NoFilter,
        );
        encoder.write_image(
            &image.as_raw(),
            image.width(),
            image.height(),
            image::ExtendedColorType::Rgba8,
        ).expect("TODO: panic message");

        tt.time_since_last_marker(); // DEBUG

        Ok(())
    }
}


fn hbitmap_to_bgr_(hbitmap: HBITMAP) -> (Vec<u8>, u32, u32) {
    unsafe {
        let mut bitmap: BITMAP = mem::zeroed();
        GetObjectW(hbitmap as *mut _, size_of::<BITMAP>() as i32, &mut bitmap as *mut _ as *mut _);

        let width = bitmap.bmWidth as u32;
        let height = bitmap.bmHeight as u32;
        let bpp = bitmap.bmBitsPixel;
        assert!(bpp == 24 || bpp == 32, "Unsupported bit depth (must be 24/32 bpp)");

        let mut info = BITMAPINFOHEADER {
            biSize: size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: bitmap.bmWidth,
            biHeight: -bitmap.bmHeight, // Top-down DIB
            biPlanes: 1,
            biBitCount: 24,
            biCompression: BI_RGB,
            biSizeImage: 0,
            biXPelsPerMeter: 0,
            biYPelsPerMeter: 0,
            biClrUsed: 0,
            biClrImportant: 0,
        };

        let mut buffer: Vec<u8> = Vec::with_capacity((width * height * 3) as usize);
        let hdc = CreateCompatibleDC(ptr::null_mut());

        GetDIBits(
            hdc,
            hbitmap,
            0,
            height,
            buffer.as_mut_ptr() as *mut _,
            &mut info as *mut _ as *mut _,
            DIB_RGB_COLORS,
        );

        DeleteDC(hdc);

        // (Windows GDI returns BGR order)

        (buffer, width, height)
    }
}

fn hbitmap_to_bgr(hbitmap: HBITMAP) -> Result<(Vec<u8>, u32, u32), String> {
    // Retrieve BITMAP information to get dimensions
    let mut bm: BITMAP = unsafe { mem::zeroed() };
    let retrieved = unsafe {
        GetObjectW(
            hbitmap as *mut _,
            mem::size_of::<BITMAP>() as i32,
            &mut bm as *mut _ as *mut _,
        )
    };
    if retrieved == 0 {
        return Err("Failed to retrieve BITMAP info".into());
    }

    // Create a compatible DC and select the HBITMAP
    let hdc_screen = unsafe { GetDC(ptr::null_mut()) };
    if hdc_screen.is_null() {
        return Err("Failed to get screen DC".into());
    }
    let hdc_mem = unsafe { CreateCompatibleDC(hdc_screen) };
    unsafe { ReleaseDC(ptr::null_mut(), hdc_screen) };
    if hdc_mem.is_null() {
        return Err("Failed to create compatible DC".into());
    }
    let old_bmp = unsafe { SelectObject(hdc_mem, hbitmap as *mut _) };
    if old_bmp.is_null() {
        unsafe { DeleteDC(hdc_mem) };
        return Err("Failed to select HBITMAP into DC".into());
    }

    // Configure BITMAPINFO for 32-bit BGRX (top-down)
    let mut bmi: BITMAPINFO = unsafe { mem::zeroed() };
    bmi.bmiHeader.biSize = size_of::<BITMAPINFOHEADER>() as u32;
    bmi.bmiHeader.biWidth = bm.bmWidth;
    bmi.bmiHeader.biHeight = -bm.bmHeight; // Negative for top-down
    bmi.bmiHeader.biPlanes = 1;
    bmi.bmiHeader.biBitCount = 32;
    bmi.bmiHeader.biCompression = BI_RGB;
    bmi.bmiHeader.biSizeImage = 0;

    // Allocate buffer to hold pixel data (32-bit BGRX)
    let buffer_size = (bm.bmWidth * bm.bmHeight * 4) as usize;
    let mut buffer = vec![0u8; buffer_size];

    // Retrieve the pixel data into buffer
    let lines = unsafe {
        GetDIBits(
            hdc_mem,
            hbitmap,
            0,
            bm.bmHeight as u32,
            buffer.as_mut_ptr() as *mut _,
            &mut bmi as *mut _ as *mut BITMAPINFO,
            DIB_RGB_COLORS,
        )
    };
    if lines == 0 {
        unsafe {
            SelectObject(hdc_mem, old_bmp);
            DeleteDC(hdc_mem);
        }
        return Err("Failed to retrieve bitmap bits".into());
    }

    // Cleanup: Restore DC and delete
    unsafe {
        SelectObject(hdc_mem, old_bmp);
        DeleteDC(hdc_mem);
    }

    // Convert from 32-bit BGRX to RGB
    let mut rgb = Vec::with_capacity((bm.bmWidth * bm.bmHeight * 3) as usize);
    for pixel in buffer.chunks_exact(4) {
        // BGRX format: [Blue, Green, Red, X]
        rgb.push(pixel[2]); // Red
        rgb.push(pixel[1]); // Green
        rgb.push(pixel[0]); // Blue
    }

    Ok((rgb, bm.bmWidth as u32, bm.bmHeight as u32))
}

fn rgb_to_yuv(rgb: &[u8], width: u32, height: u32) -> Vec<u8>{
    convert_rgb_to_yuv420p(&rgb, width, height, 3)
}

pub fn hbitmaps_to_h264(hbitmaps: &Vec<HBITMAP>, fps: u32) -> Result<(), String> {
    let config = EncoderConfig::new()
        .bitrate(BitRate::from_bps(5_000_000))
        .intra_frame_period(IntraFramePeriod::from_num_frames(fps))
        .num_threads(2)
        .qp(QpRange::new(22, 38))
        .skip_frames(false);


    let mut encoder = Encoder::with_api_config(OpenH264API::from_source(), config)
        .map_err(|e| format!("Encoder creation failed: {}", e))?;

    let file = File::create("out/output.h264")
        .map_err(|e| format!("File creation failed: {}", e))?;

    let mut writer = BufWriter::new(file);

    for hbitmap in hbitmaps {
        let (brg, _width, _height) = hbitmap_to_bgr(*hbitmap)
            .map_err(|e| format!("File creation failed: {}", e))?;;
        println!("{}, {}, {}", brg.len(), _width, _height);
        let brg_slice_u8 = BgrSliceU8::new(&brg, (_width as usize, _height as usize));
        let yuv_buffer = YUVBuffer::from_rgb_source(brg_slice_u8);
        let encoded_bit_stream = encoder.encode(&yuv_buffer)
            .map_err(|e| format!("Encoding failed: {}", e))?;
        encoded_bit_stream.write(&mut writer)
            .map_err(|e| format!("Writing failed: {}", e))?;
    }

    Ok(())
}