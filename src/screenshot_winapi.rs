use std::{thread, time::Duration, ptr};
use windows::Win32::Graphics::Dxgi::*;
use winapi::{
    shared::{windef::{HBITMAP, HGDIOBJ}},
    um::{
        wingdi::{
            BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC, DeleteObject,
            GetDeviceCaps, SelectObject, SRCCOPY, HORZRES, VERTRES,
        },
        winuser::{GetDC, ReleaseDC},
    },
};

pub fn capture_screen() -> Result<(HBITMAP, i32, i32), ()> {
    unsafe {
        // Get the screen DC
        let h_screen_dc = GetDC(ptr::null_mut());
        if h_screen_dc.is_null() {
            return Err(());
        }

        // Create memory DC compatible with screen DC
        let h_memory_dc = CreateCompatibleDC(h_screen_dc);
        if h_memory_dc.is_null() {
            ReleaseDC(ptr::null_mut(), h_screen_dc);
            return Err(());
        }

        // Get screen dimensions
        let width = GetDeviceCaps(h_screen_dc, HORZRES);
        let height = GetDeviceCaps(h_screen_dc , VERTRES);

        // Create a compatible bitmap
        let h_bitmap = CreateCompatibleBitmap(h_screen_dc, width, height);
        if h_bitmap.is_null() {
            DeleteDC(h_memory_dc);
            ReleaseDC(ptr::null_mut(), h_screen_dc);
            return Err(());
        }

        // Select bitmap into memory DC and save old bitmap
        let h_old_bitmap = SelectObject(h_memory_dc, h_bitmap as HGDIOBJ);
        if h_old_bitmap.is_null() {
            DeleteObject(h_bitmap as HGDIOBJ);
            DeleteDC(h_memory_dc);
            ReleaseDC(ptr::null_mut(), h_screen_dc);
            return Err(());
        }

        // Copy screen content to bitmap
        // Perform bit block transfer
        if BitBlt(h_memory_dc, 0, 0, width, height, h_screen_dc, 0, 0, SRCCOPY) == 0 {
            // Cleanup on failure
            SelectObject(h_memory_dc, h_old_bitmap);
            DeleteObject(h_bitmap as HGDIOBJ);
            DeleteDC(h_memory_dc);
            ReleaseDC(ptr::null_mut(), h_screen_dc);
            return Err(());
        }

        // Select old bitmap back into memory DC
        let h_bitmap = SelectObject(h_memory_dc, h_old_bitmap) as HBITMAP;

        // Cleanup DCs
        DeleteDC(h_memory_dc);
        ReleaseDC(ptr::null_mut(), h_screen_dc);

        // h_bitmap now contains the screenshot
        Ok((h_bitmap as HBITMAP, width, height))
    }
}

pub fn capture_screens(amount: usize, fps: u32) -> Result<(Vec<HBITMAP>, i32, i32), ()> {
    unsafe {
        // Get the screen DC
        let h_screen_dc = GetDC(ptr::null_mut());
        if h_screen_dc.is_null() {
            return Err(());
        }

        // Create memory DC compatible with screen DC
        let h_memory_dc = CreateCompatibleDC(h_screen_dc);
        if h_memory_dc.is_null() {
            ReleaseDC(ptr::null_mut(), h_screen_dc);
            return Err(());
        }

        // Get screen dimensions
        let width = GetDeviceCaps(h_screen_dc, HORZRES);
        let height = GetDeviceCaps(h_screen_dc, VERTRES);

        let mut screenshots = Vec::with_capacity(amount);
        let mut error_occurred = false;

        // Store original bitmap in memory DC
        let h_default_bitmap = SelectObject(h_memory_dc, CreateCompatibleBitmap(h_screen_dc, 1, 1) as HGDIOBJ);

        for i in 0..amount {
            // Create new bitmap for this frame
            let h_bitmap = CreateCompatibleBitmap(h_screen_dc, width, height);
            if h_bitmap.is_null() {
                error_occurred = true;
                break;
            }

            // Select new bitmap into memory DC
            let h_old_bitmap = SelectObject(h_memory_dc, h_bitmap as HGDIOBJ);
            if h_old_bitmap.is_null() {
                DeleteObject(h_bitmap as HGDIOBJ);
                error_occurred = true;
                break;
            }

            // Capture screen
            if BitBlt(
                h_memory_dc,
                0,
                0,
                width,
                height,
                h_screen_dc,
                0,
                0,
                SRCCOPY,
            ) == 0
            {
                SelectObject(h_memory_dc, h_old_bitmap);
                DeleteObject(h_bitmap as HGDIOBJ);
                error_occurred = true;
                break;
            }

            // Restore previous bitmap and store captured frame
            SelectObject(h_memory_dc, h_old_bitmap);
            screenshots.push(h_bitmap);

            // Sleep between captures (except after last)
            /*if i < (amount - 1) {
                thread::sleep(Duration::from_millis((1000 / fps) as u64));
            }*/
        }

        // Cleanup GDI resources
        SelectObject(h_memory_dc, h_default_bitmap);
        DeleteDC(h_memory_dc);
        ReleaseDC(ptr::null_mut(), h_screen_dc);

        if error_occurred {
            // Cleanup any captured bitmaps
            for hbmp in screenshots {
                DeleteObject(hbmp as HGDIOBJ);
            }
            Err(())
        } else {
            Ok((screenshots, width, height))
        }
    }
}
