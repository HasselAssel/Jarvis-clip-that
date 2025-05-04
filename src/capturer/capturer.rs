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
    pub fn new(fps: i32, out_width: u32, out_height: u32, ring_buffer: Arc<Mutex<RingBuffer>>) -> (Self, Arc<Mutex<ffmpeg_next::codec::encoder::Video>>) {
        let codec = ffmpeg_next::codec::encoder::find(ffmpeg_next::codec::id::Id::H265).or_else(|| {
            println!("H264 not found :(");
            ffmpeg_next::codec::encoder::find_by_name("libx264")
        }).ok_or(ffmpeg_next::Error::EncoderNotFound).unwrap();
        let ctx = ffmpeg_next::codec::context::Context::new_with_codec(codec);
        let mut enc = ctx.encoder().video().unwrap();

        enc.set_width(out_width);
        enc.set_height(out_height);
        enc.set_format(ffmpeg_next::format::Pixel::YUV420P);
        enc.set_time_base((1, fps));
        enc.set_frame_rate(Some((fps, 1)));
        enc.set_bit_rate(8_000_000);

        enc.set_flags(ffmpeg_next::codec::Flags::GLOBAL_HEADER); // Extradata is generated
        enc.set_gop(fps as u32); // Keyframe interval (1 second)

        let _v = enc.open_as(codec).unwrap();
        let video_encoder = Arc::new(Mutex::new(_v));
        let video_encoder_return = Arc::clone(&video_encoder);

        let mut device: Option<ID3D11Device> = None;
        let mut context: Option<ID3D11DeviceContext> = None;
        // hmmm maybe safe, maybe unsafe, who knows
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

        (Self {
            fps,
            out_width,
            out_height,
            ring_buffer,
            video_encoder,

            duplication,
            context,
            single_texture_buffer,
            single_texture_desc,
        },
         video_encoder_return)
    }

    pub fn start_capturing(self) -> JoinHandle<Result<()>>{
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
            let scanline_bytes = self.single_texture_desc.Width * bytes_per_pixel;
            let mut src_frame = ffmpeg_next::util::frame::video::Video::new(ffmpeg_next::format::Pixel::BGRA, self.single_texture_desc.Width, self.single_texture_desc.Height);
            let mut dst_frame = ffmpeg_next::util::frame::video::Video::new(ffmpeg_next::format::Pixel::YUV420P, self.out_width, self.out_height);

            let mut piped_frames = 0;

            let mut frame_counter = 0;

            let mut t_encode = Instant::now();

            loop {
                start_time = Instant::now();

                for i in 0..u32::MAX {
                    let mut t_acquire = Instant::now();
                    elapsed = start_time.elapsed();

                    expected_elapsed = frame_duration.saturating_mul(i);
                    if expected_elapsed > elapsed {
                        println!("SLEEPY TIME");
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

                                eprintln!("GPU copy: {:?}", t_acquire.elapsed());
                                let mut t_copy = Instant::now();

                                if hr.is_err() {
                                    panic!("HEY MAPPING DIDNT WORK");
                                }

                                scaler = ffmpeg_next::software::scaling::context::Context::get(
                                    ffmpeg_next::format::Pixel::BGRA, // Source pixel format
                                    self.single_texture_desc.Width,
                                    self.single_texture_desc.Height,
                                    ffmpeg_next::format::Pixel::YUV420P, // Destination pixel format
                                    self.out_width,
                                    self.out_height,
                                    ffmpeg_next::software::scaling::Flags::BILINEAR,
                                ).unwrap();

                                let base_ptr = mapped.pData as *const u8;
                                let base = NonNull::new(base_ptr as *mut u8)
                                    .expect("Mapped pData should never be null");

                                frame_buffer = Vec::with_capacity((self.single_texture_desc.Width * self.single_texture_desc.Height * 4) as usize);
                                for row in 0..self.single_texture_desc.Height {
                                    let offset = row * mapped.RowPitch;
                                    // SAFETY: each scanline is contiguous memory of length scanline_bytes
                                    let slice = unsafe { std::slice::from_raw_parts(base.as_ptr().add(offset as usize), scanline_bytes as usize) };
                                    frame_buffer.extend_from_slice(slice);
                                }
                                let src_data = src_frame.data_mut(0);
                                src_data.copy_from_slice(&frame_buffer);

                                eprintln!("CPU copy: {:?}", t_copy.elapsed());
                                let mut t_scale = Instant::now();

                                scaler.run(&src_frame, &mut dst_frame).unwrap();

                                eprintln!("Scaling: {:?}", t_scale.elapsed());

                                t_encode = Instant::now();
                            }
                        } else {
                            panic!("somehow None :(");
                        }

                        // how can releasing a frame not be safe, right?
                        unsafe { self.duplication.ReleaseFrame().unwrap(); }
                    } else {
                        println!("ERROR!");
                    }
                    println!("{}, {}", frame_counter, elapsed.as_millis());

                    dst_frame.set_pts(Some(frame_counter));
                    frame_counter += 1;

                    let mut video_encoder = self.video_encoder.lock().unwrap();
                    video_encoder.send_frame(&dst_frame).unwrap();
                    piped_frames += 1;

                    let mut packet: ffmpeg_next::codec::packet::Packet = ffmpeg_next::codec::packet::Packet::empty();
                    let mut ring_buffer = self.ring_buffer.lock().unwrap();
                    while let Ok(_) = video_encoder.receive_packet(&mut packet) {
                        ring_buffer.insert(PacketWrapper::new(piped_frames, packet.clone()));
                        piped_frames = 0;
                        packet = ffmpeg_next::codec::packet::Packet::empty();
                        println!("RECEIVER");
                    }
                    drop(ring_buffer);
                    drop(video_encoder);

                    eprintln!("Encode+I/O: {:?}", t_encode.elapsed());
                }
            }
        })
    }
}