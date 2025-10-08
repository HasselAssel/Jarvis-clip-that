use windows::{
    core::*,
    Graphics::{
        Capture::*,
        DirectX::*,
        DirectX::Direct3D11::*,
        Imaging::*,
    },
    Win32::Foundation::*,
    Win32::Graphics::Direct3D11::*,
    Win32::UI::WindowsAndMessaging::*,
};

use std::fs::File;
use std::io::Write;
use windows::Win32::Storage::Xps::{PRINT_WINDOW_FLAGS, PrintWindow};
use windows::Win32::UI::WindowsAndMessaging::FindWindowW;
use windows::Win32::Graphics::Direct3D::{D3D_DRIVER_TYPE_HARDWARE, D3D_FEATURE_LEVEL_10_0, D3D_FEATURE_LEVEL_11_0};
use windows::Win32::Graphics::Dxgi::IDXGIDevice;
use windows::Win32::Graphics::Gdi::{GetDIBits, BITMAPINFO, BITMAPINFOHEADER, CreateCompatibleBitmap, CreateCompatibleDC, GetDC, HBITMAP, ReleaseDC, SelectObject, DIB_RGB_COLORS, BITMAP, GetObjectW};

pub fn main() -> Result<()> {
    unsafe {
        // Find Notepad window (change title for Firefox etc.)
        let hwnd = FindWindowW(None, w!("Spotify Premium"));

        let hwnd = if let Ok(hwnd) = hwnd {
            if hwnd.is_invalid() {
                panic!("Window not found!");
            }
            hwnd
        } else {
            panic!("Returned Error!");
        };

        capture_window_printwindow(hwnd);

    }
    Ok(())
}

use image::{ImageBuffer, Rgba};


fn capture_window_printwindow(hwnd: HWND) {
    unsafe {
        // Get window size
        let mut rect = windows::Win32::Foundation::RECT::default();
        GetClientRect(hwnd, &mut rect).unwrap();

        // Create bitmap and DC
        let hdc_screen = GetDC(Some(hwnd));
        let hdc_mem = CreateCompatibleDC(Some(hdc_screen));
        let bmp = CreateCompatibleBitmap(hdc_screen, rect.right, rect.bottom);
        SelectObject(hdc_mem, bmp.into());

        // Call PrintWindow
        if PrintWindow(hwnd, hdc_mem, PRINT_WINDOW_FLAGS(0)).as_bool() {
            println!("✅ PrintWindow succeeded");
            // You can now save the HBITMAP as PNG using GDI+ or Windows Imaging Component
        } else {
            println!("❌ PrintWindow failed");
        }

        let (width, height) = get_bitmap_size(bmp);

        let pixels = bitmap_to_vec(bmp, width, height);
        let img: ImageBuffer<Rgba<u8>, _> = ImageBuffer::from_raw(width as u32, height as u32, pixels)
            .expect("Buffer size mismatch");
        img.save("out/screenshot.png").unwrap();
    }

}

unsafe fn bitmap_to_vec(hbitmap: HBITMAP, width: i32, height: i32) -> Vec<u8> {
    let mut bmi = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: width,
            biHeight: -height, // top-down
            biPlanes: 1,
            biBitCount: 32,
            biCompression: 0,
            ..Default::default()
        },
        ..Default::default()
    };

    let mut buffer = vec![0u8; (width * height * 4) as usize];
    let hdc = GetDC(None);
    GetDIBits(hdc, hbitmap, 0, height as u32, Some(buffer.as_mut_ptr() as *mut _), &mut bmi, DIB_RGB_COLORS);
    ReleaseDC(None, hdc);

    buffer
}

unsafe fn get_bitmap_size(hbitmap: HBITMAP) -> (i32, i32) {
    let mut bmp = BITMAP::default();
    if GetObjectW(hbitmap.into(), std::mem::size_of::<BITMAP>() as i32, Some(&mut bmp as *mut _ as *mut _)) == 0 {
        panic!("GetObject failed");
    }
    (bmp.bmWidth, bmp.bmHeight)
}