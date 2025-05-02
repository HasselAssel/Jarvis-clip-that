use std::ptr::NonNull;
use std::sync::{Arc, Mutex};
use std::thread;
use std::thread::{JoinHandle, sleep};
use std::time::{Duration, Instant};

use windows::core::{Result, Interface};
use windows::Win32::Foundation::HMODULE;
use windows::Win32::Graphics::Direct3D::{D3D_DRIVER_TYPE_HARDWARE, D3D_FEATURE_LEVEL_10_0, D3D_FEATURE_LEVEL_11_0};
use windows::Win32::Graphics::Direct3D11::{D3D11_CPU_ACCESS_READ, D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_MAP_READ, D3D11_MAPPED_SUBRESOURCE, D3D11_SDK_VERSION, D3D11_TEXTURE2D_DESC, D3D11_USAGE_STAGING, D3D11CreateDevice, ID3D11Device, ID3D11DeviceContext, ID3D11Texture2D};
use windows::Win32::Graphics::Dxgi::{DXGI_OUTDUPL_DESC, DXGI_OUTDUPL_FRAME_INFO, IDXGIAdapter, IDXGIDevice, IDXGIOutput, IDXGIOutput1, IDXGIOutputDuplication, IDXGIResource};
use windows::Win32::Graphics::Dxgi::Common::{DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_SAMPLE_DESC};

use crate::capturer::ring_buffer::{RingBuffer, PacketWrapper};

struct VieleWerte {
}

pub struct Capturer {
    fps: i32,
    out_width: u32,
    out_height: u32,

    ring_buffer: Arc<Mutex<RingBuffer>>,
    video_encoder: Arc<Mutex<ffmpeg_next::codec::encoder::Video>>,

    duplication: IDXGIOutputDuplication,
    context: ID3D11DeviceContext,
    single_texture_buffer: ID3D11Texture2D,
    single_texture_desc: D3D11_TEXTURE2D_DESC,
}

impl Capturer {
    pub fn new(fps: i32, out_width: u32, out_height: u32, video_encoder: Arc<Mutex<ffmpeg_next::codec::encoder::Video>>, ring_buffer: Arc<Mutex<RingBuffer>>) -> Self {
        let mut device: Option<ID3D11Device> = None;
        let mut context: Option<ID3D11DeviceContext> = None;
        // hmmm maybe unsafe, maybe safe who knows
        unsafe {
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
        }
        let device: ID3D11Device = device.unwrap();
        let context: ID3D11DeviceContext = context.unwrap();
        let dxgi_device: IDXGIDevice = device.cast().unwrap();

        let adapter: IDXGIAdapter;
        let output: IDXGIOutput;
        let output1: IDXGIOutput1;
        let duplication: IDXGIOutputDuplication;
        let out_desc: DXGI_OUTDUPL_DESC;
        // yea IDK why this is safe, works on my machine
        unsafe {
            adapter = dxgi_device.GetAdapter().unwrap();
            // Enumerate the first output (primary monitor).
            output = adapter.EnumOutputs(0).unwrap();
        }
        output1 = output.cast().unwrap();
        // same here, dont know, dont care
        unsafe {
            duplication = output1.DuplicateOutput(&device).unwrap();
            out_desc = duplication.GetDesc();
        }
        let _width: u32 = out_desc.ModeDesc.Width;
        let _height: u32 = out_desc.ModeDesc.Height;

        let single_texture_desc = D3D11_TEXTURE2D_DESC {
            Width: _width,
            Height: _height,
            MipLevels: 1,
            ArraySize: 1,
            Format: DXGI_FORMAT_B8G8R8A8_UNORM,
            SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
            Usage: D3D11_USAGE_STAGING,
            BindFlags: 0,
            CPUAccessFlags: D3D11_CPU_ACCESS_READ.0 as u32,
            MiscFlags: 0,
        };

        let single_texture_buffer: ID3D11Texture2D = {
            let mut dest_texture = None;
            // hmmmmmmmmmmmmmmmmmmmmmmmmmmmmmmmmmmm...
            unsafe { device.CreateTexture2D(&single_texture_desc, None, Some(&mut dest_texture)).unwrap(); }
            dest_texture.unwrap()
        };

        Self {
            fps,
            out_width,
            out_height,
            ring_buffer,
            video_encoder,

            duplication,
            context,
            single_texture_buffer,
            single_texture_desc,
        }
    }

    pub fn start_capturing(self) -> JoinHandle<Result<()>>{
        let fps = self.fps;
        let width = self.out_width;
        let height = self.out_height;

        let arc_video_encoder = Arc::clone(&self.video_encoder);
        let arc_ring_buffer: Arc<Mutex<RingBuffer>> = Arc::clone(&self.ring_buffer);
        let desc_width = self.single_texture_desc.Width;
        let desc_height = self.single_texture_desc.Height;

        thread::spawn(move || -> Result<()> {
            let mut resource: Option<IDXGIResource>;
            let mut hr: Result<()>;
            let mut frame_info: DXGI_OUTDUPL_FRAME_INFO = DXGI_OUTDUPL_FRAME_INFO::default();
            let mut tex: ID3D11Texture2D;
            let mut mapped: D3D11_MAPPED_SUBRESOURCE;

            let frame_duration: Duration = Duration::from_secs_f64(1.0f64 / (self.fps as f64));
            let mut elapsed: Duration;
            let mut expected_elapsed: Duration;
            let mut start_time: Instant;

            let mut scaler: ffmpeg_next::software::scaling::context::Context;
            let mut frame_buffer: Vec<u8>;
            let bytes_per_pixel = 4;
            let scanline_bytes = desc_width * bytes_per_pixel;
            let mut src_frame = ffmpeg_next::util::frame::video::Video::new(ffmpeg_next::format::Pixel::BGRA, desc_width, desc_height);
            let mut dst_frame = ffmpeg_next::util::frame::video::Video::new(ffmpeg_next::format::Pixel::YUV420P, self.out_width, self.out_height);

            let mut piped_frames = 0;

            let mut frame_counter = 0;

            loop {
                start_time = Instant::now();

                for i in 0..u32::MAX {
                    elapsed = start_time.elapsed();

                    expected_elapsed = frame_duration.saturating_mul(i);
                    if expected_elapsed > elapsed {
                        sleep(expected_elapsed - elapsed);
                    }
                    resource = None;
                    // surely this is safe!
                    unsafe { hr = self.duplication.AcquireNextFrame(0, &mut frame_info, &mut resource); }
                    if !hr.is_err() {
                        if let Some(dxgi_resource) = resource {
                            if frame_info.AccumulatedFrames != 0 {
                                tex = dxgi_resource.cast().unwrap();

                                mapped = D3D11_MAPPED_SUBRESOURCE::default();
                                //let hr: Result<()>;
                                // IT'S SAFE, IT'S SAFE, IT'S SAFE
                                unsafe {
                                    self.context.CopyResource(&self.single_texture_buffer, &tex);
                                    hr = self.context.Map(&self.single_texture_buffer, 0, D3D11_MAP_READ, 0, Some(&mut mapped));
                                }

                                if hr.is_err() {
                                    panic!("HEY MAPPING DIDNT WORK");
                                }

                                scaler = ffmpeg_next::software::scaling::context::Context::get(
                                    ffmpeg_next::format::Pixel::BGRA, // Source pixel format
                                    desc_width,
                                    desc_height,
                                    ffmpeg_next::format::Pixel::YUV420P, // Destination pixel format
                                    self.out_width,
                                    self.out_height,
                                    ffmpeg_next::software::scaling::Flags::FAST_BILINEAR,
                                ).unwrap();

                                let base_ptr = mapped.pData as *const u8;
                                let base = NonNull::new(base_ptr as *mut u8)
                                    .expect("Mapped pData should never be null");

                                frame_buffer = Vec::with_capacity((desc_width * desc_height * 4) as usize);
                                for row in 0..desc_height {
                                    let offset = row * mapped.RowPitch;
                                    // SAFETY: we know each scanline is contiguous memory of length scanline_bytes
                                    let slice = unsafe { std::slice::from_raw_parts(base.as_ptr().add(offset as usize), scanline_bytes as usize) };
                                    frame_buffer.extend_from_slice(slice);
                                }
                                let src_data = src_frame.data_mut(0);
                                src_data.copy_from_slice(&frame_buffer);

                                scaler.run(&src_frame, &mut dst_frame).unwrap();
                            }
                        } else {
                            panic!("somehow None :(");
                        }

                        // how can releasing a frame not be safe, right?
                        unsafe { self.duplication.ReleaseFrame().unwrap(); }
                    }

                    dst_frame.set_pts(Some(frame_counter as i64));
                    frame_counter += 1;

                    let mut video_encoder = arc_video_encoder.lock().unwrap();
                    video_encoder.send_frame(&dst_frame).unwrap();
                    piped_frames += 1;

                    let mut packet: ffmpeg_next::codec::packet::Packet = ffmpeg_next::codec::packet::Packet::empty();
                    while let Ok(_) = video_encoder.receive_packet(&mut packet) {
                        let mut ring_buffer = arc_ring_buffer.lock().unwrap();
                        ring_buffer.insert(PacketWrapper::new(piped_frames, packet.clone()));
                        piped_frames = 0;
                        packet = ffmpeg_next::codec::packet::Packet::empty();
                    }
                    drop(video_encoder);
                }
            }
        })
    }
}