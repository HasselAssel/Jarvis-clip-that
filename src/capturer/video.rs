use std::sync::{Mutex, RwLock};
use windows::Win32::Graphics::Direct3D11::{D3D11_MAP_READ, D3D11_MAPPED_SUBRESOURCE, D3D11_TEXTURE2D_DESC, ID3D11DeviceContext};
use crate::capturer::ring_buffer_old::{OptionalID3D11Texture2D, OptionalID3D11Texture2DRingBuffer};
use std::process::{Command, Stdio};
use std::io::Write;
use std::ptr::NonNull;
use std::time::Instant;
pub struct VideoData {}

impl VideoData {
    fn write_id3d11texture2d_to_mp4(mut_context: &Mutex<ID3D11DeviceContext>, textures: &[RwLock<OptionalID3D11Texture2D>], width: u32, height: u32, fps: u32, output_path: &str) -> std::io::Result<()> {
        let mut ffmpeg = Command::new("ffmpeg")
            .args([
                "-f", "rawvideo",
                "-pixel_format", "bgra",
                "-video_size", &format!("{}x{}", width, height),
                "-framerate", &fps.to_string(),
                "-i", "pipe:0",
                "-c:v", "libx264",
                "-preset", "ultrafast",
                "-tune", "zerolatency",
                "-crf", "0",
                "-pix_fmt", "yuv420p",
                "-y", output_path,
            ])
            .stdin(Stdio::piped())
            .spawn()?;

        let stdin = ffmpeg.stdin.as_mut().unwrap();

        let time_s = Instant::now();
        for texture_lock in textures {
            let mut desc = D3D11_TEXTURE2D_DESC::default();

            let texture_guard = texture_lock.read().unwrap();
            let texture = texture_guard.get_tex();

            unsafe {
                texture.GetDesc(&mut desc);
            }

            let width = desc.Width;
            let height = desc.Height;
            let pixel_size = 4; // For DXGI_FORMAT_B8G8R8A8_UNORM. without A :(

            // Map the texture to access its data on the CPU.
            let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
            let hr: windows::core::Result<()>;
            let context = mut_context.lock().unwrap();
            unsafe {
                hr = context.Map(texture, 0, D3D11_MAP_READ, 0, Some(&mut mapped));
            }
            drop(texture_guard);
            drop(context);
            if hr.is_err() {
                println!("ERRRRRRRRRRRRRRRROOOOOOOORRRRRRRR: {:?}", hr);
            }

            let bytes_per_pixel = 4;
            let scanline_bytes = width * bytes_per_pixel;
            let base_ptr = mapped.pData as *const u8;
            let base = NonNull::new(base_ptr as *mut u8)
                .expect("Mapped pData should never be null");
            let mut frame_buffer = Vec::with_capacity((width * height * 4) as usize);
            for row in 0..height {
                let offset = row * mapped.RowPitch;
                // SAFETY: we know each scanline is contiguous memory of length scanline_bytes
                let slice = unsafe {
                    std::slice::from_raw_parts(base.as_ptr().add(offset as usize), scanline_bytes as usize)
                };
                frame_buffer.extend_from_slice(slice);
            }
            stdin.write_all(&frame_buffer)?;
        }
        println!("FEEDING TIME {}", time_s.elapsed().as_millis());

        ffmpeg.wait()?; // Wait for ffmpeg to finish

        Ok(())
    }

    pub fn save_as_mp4<const BUFFER_LEN: usize>(mut_context: &Mutex<ID3D11DeviceContext>, textures: &OptionalID3D11Texture2DRingBuffer<BUFFER_LEN>, fps: u32) -> Result<(), ffmpeg_next::util::error::Error> {
        let tex_desc = textures.get_tex_desc();
        let width = tex_desc.Width;
        let height = tex_desc.Height;

        /*let rgb_vec: Vec<Vec<u8>> = textures.iter().map(|tex| {
            let _tex = tex.read().unwrap();
            Self::id3d11texture2d_to_rgba(mut_context, &_tex.get_tex())
        }).collect();*/

        println!("gay");

        /*let img_buffer = ImageBuffer::<Rgba<u8>, _>::from_raw(width, height, rgb_vec[0].as_slice())
            .ok_or_else(|| {
                Error::new(windows::core::HRESULT(0), "Failed to create image buffer")
            }).unwrap();
        img_buffer.save("out/NIGGER.png").unwrap();*/

        println!("hi");

        let _ =  Self::write_id3d11texture2d_to_mp4(mut_context, textures.as_slice(), width, height, fps, "out/output.mp4");

        Ok(())
    }
}