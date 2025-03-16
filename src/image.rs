use std::{fs::File, io::BufWriter, mem, ptr};
use std::mem::size_of;
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


pub fn hbitmap_to_png(hbitmap: HBITMAP, output_path: &str) -> Result<(), String> {
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

        let mut bitmap_info = BITMAPINFO {
            bmiHeader: bmi,
            bmiColors: [mem::zeroed()], // Initialize with zeroed RGBQUAD
        };

        // Create buffer for pixel data
        let buffer_size = (width * height * 4) as usize;
        let mut buffer: Vec<u8> = vec![0; buffer_size];

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

        // Convert BGRA to RGBA and handle alpha channel
        for chunk in buffer.chunks_exact_mut(4) {
            // Swap red and blue channels (BGRA -> RGBA)
            chunk.swap(0, 2);

            // Set alpha to 255 if source was 24bpp
            if bpp == 24 {
                chunk[3] = 255;
            }
        }

        // Create image and save as PNG
        let image: RgbaImage = ImageBuffer::from_raw(
            width as u32,
            height as u32,
            buffer,
        ).ok_or("Failed to create image buffer")?;

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

        Ok(())
    }
}