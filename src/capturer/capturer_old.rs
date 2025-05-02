use std::cell::RefCell;
use std::ptr::NonNull;
use std::sync::{Arc, Mutex};
use std::thread;
use std::thread::{sleep};
use std::time::{Duration, Instant};
use ffmpeg_next::{codec, Error as FfmpegError};

use windows::core::{Result, Interface};
use windows::Win32::Foundation::HMODULE;
use windows::Win32::Graphics::Direct3D::{D3D_DRIVER_TYPE_HARDWARE, D3D_FEATURE_LEVEL_10_0, D3D_FEATURE_LEVEL_11_0};
use windows::Win32::Graphics::Direct3D11::{D3D11_BIND_SHADER_RESOURCE, D3D11_CPU_ACCESS_READ, D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_MAP_READ, D3D11_MAPPED_SUBRESOURCE, D3D11_SDK_VERSION, D3D11_TEXTURE2D_DESC, D3D11_USAGE_DEFAULT, D3D11_USAGE_STAGING, D3D11CreateDevice, ID3D11Device, ID3D11DeviceContext, ID3D11Texture2D};
use windows::Win32::Graphics::Dxgi::{DXGI_OUTDUPL_DESC, DXGI_OUTDUPL_FRAME_INFO, IDXGIAdapter, IDXGIDevice, IDXGIOutput, IDXGIOutput1, IDXGIOutputDuplication, IDXGIResource};
use windows::Win32::Graphics::Dxgi::Common::{DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_SAMPLE_DESC};

use crate::capturer::ring_buffer_old::OptionalID3D11Texture2DRingBuffer;
use crate::capturer::video::VideoData;
use crate::capturer::ring_buffer::RingBuffer;

pub struct Capturer<const BUFFER_LEN: usize, const STANDARD_OUT_LEN: usize> {
    duplication: RefCell<Option<IDXGIOutputDuplication>>,
    context: Arc<Mutex<ID3D11DeviceContext>>,

    fps: u32,

    ring_buffer_old: Arc<OptionalID3D11Texture2DRingBuffer<BUFFER_LEN>>,
    standard_out_buffer: OptionalID3D11Texture2DRingBuffer<STANDARD_OUT_LEN>,

    ring_buffer: RingBuffer,


    single_texture_buffer: Arc<Mutex<ID3D11Texture2D>>,
    single_texture_desc: Arc<Mutex<D3D11_TEXTURE2D_DESC>>,
}

impl<const BUFFER_LEN: usize, const STANDARD_OUT_LEN: usize> Capturer<BUFFER_LEN, STANDARD_OUT_LEN> {
    pub fn new(fps: u32) -> Self {
        assert!(BUFFER_LEN >= STANDARD_OUT_LEN);

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
        let width: u32 = out_desc.ModeDesc.Width;
        let height: u32 = out_desc.ModeDesc.Height;

        let buffer_tex_desc = D3D11_TEXTURE2D_DESC {
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

        let out_buffer_tex_desc = D3D11_TEXTURE2D_DESC {
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
        let ring_buffer_old: Arc<OptionalID3D11Texture2DRingBuffer<BUFFER_LEN>> = Arc::new(OptionalID3D11Texture2DRingBuffer::<BUFFER_LEN>::new(&device, out_buffer_tex_desc));// !!!!!!!!!!!!!!
        let standard_out_buffer: OptionalID3D11Texture2DRingBuffer<STANDARD_OUT_LEN> = OptionalID3D11Texture2DRingBuffer::<STANDARD_OUT_LEN>::new(&device, out_buffer_tex_desc);

        let ring_buffer: RingBuffer = RingBuffer::new(10);

        let duplication: RefCell<Option<IDXGIOutputDuplication>> = RefCell::new(Some(duplication));
        let context: Arc<Mutex<ID3D11DeviceContext>> = Arc::new(Mutex::new(context));


        let single_texture_buffer: Arc<Mutex<ID3D11Texture2D>> = Arc::new(Mutex::new({
            let mut dest_texture = None;
            unsafe {
                device.CreateTexture2D(&out_buffer_tex_desc, None, Some(&mut dest_texture)).unwrap();
            }
            dest_texture.unwrap()
        }));

        Self {
            duplication,
            context,
            fps,
            ring_buffer_old,
            standard_out_buffer,
            ring_buffer,
            single_texture_buffer,
            single_texture_desc: Arc::new(Mutex::new(out_buffer_tex_desc)),
        }
    }

    pub fn start_capturing_2(&self, width: u32, height: u32) {
        ffmpeg_next::init().unwrap();
        let codec = codec::encoder::find(codec::id::Id::H264).or_else(|| codec::encoder::find_by_name("libx264")).ok_or(FfmpegError::EncoderNotFound).unwrap();
        let mut ctx = codec::context::Context::new_with_codec(codec);
        let mut enc = ctx.encoder().video().unwrap();

        // 4. Set parameters
        enc.set_width(width);
        enc.set_height(height);
        enc.set_format(ffmpeg_next::format::Pixel::YUV420P);
        enc.set_time_base((1, self.fps as i32));
        enc.set_frame_rate(Some((1, self.fps as i32)));
        enc.set_bit_rate(7_904_011);

        // 5. Open it
        let mut video_encoder = enc.open_as(codec).unwrap();

        let arc_ring_buffer: Arc<OptionalID3D11Texture2DRingBuffer<BUFFER_LEN>> = Arc::clone(&self.ring_buffer_old);
        let duplication: IDXGIOutputDuplication = self.duplication.take().expect("The Capturers Device is None");
        let arc_context: Arc<Mutex<ID3D11DeviceContext>> = Arc::clone(&self.context);
        let arc_single_texture_buffer: Arc<Mutex<ID3D11Texture2D>> = Arc::clone(&self.single_texture_buffer);
        let arc_single_texture_desc: Arc<Mutex<D3D11_TEXTURE2D_DESC>> = Arc::clone(&self.single_texture_desc);
        let fps = self.fps;

        let _ = thread::spawn(move || -> Result<()> {
            let mut resource: Option<IDXGIResource>;
            let mut hr: Result<()>;
            let mut frame_info: DXGI_OUTDUPL_FRAME_INFO = DXGI_OUTDUPL_FRAME_INFO::default();
            let mut tex: ID3D11Texture2D;

            let frame_duration: Duration = Duration::from_secs_f64(1.0f64 / (fps as f64));
            let mut elapsed: Duration;
            let mut expected_elapsed: Duration;
            let mut start_time: Instant;

            loop {
                start_time = Instant::now();

                for i in 0..u32::MAX {
                    elapsed = start_time.elapsed();

                    println!("{}: {}", i, elapsed.as_millis());

                    expected_elapsed = frame_duration.saturating_mul(i);
                    if expected_elapsed > elapsed {
                        sleep(expected_elapsed - elapsed);
                    }

                    let stime = Instant::now();

                    arc_ring_buffer.advance_index();

                    resource = None;
                    // surely this is safe!
                    unsafe {
                        hr = duplication.AcquireNextFrame(0, &mut frame_info, &mut resource);
                    }
                    if hr.is_err() {
                        //println!("why???:{:?}", hr);
                        continue;
                    }
                    if let Some(dxgi_resource) = resource {
                        if frame_info.AccumulatedFrames == 0 {
                            // how can releasing a frame not be safe, right?
                            unsafe {
                                duplication.ReleaseFrame().unwrap();
                            }
                            continue;
                        }
                        tex = dxgi_resource.cast().unwrap();

                        // Copy the acquired frame to the destination texture
                        //arc_ring_buffer.copy_in(&arc_context, &tex);
                        let context = arc_context.lock().unwrap();
                        let single_texture_buffer = arc_single_texture_buffer.lock().unwrap();



                        let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
                        let hr: Result<()>;
                        // IT'S SAFE, IT'S SAFE, IT'S SAFE
                        unsafe {
                            context.CopyResource(&*single_texture_buffer, &tex);
                            hr = context.Map(&*single_texture_buffer, 0, D3D11_MAP_READ, 0, Some(&mut mapped));
                        }
                        drop(single_texture_buffer);
                        drop(context);

                        if hr.is_err() {
                            println!("ERRRRRRRRRRRRRRRROOOOOOOORRRRRRRR: {:?}", hr);
                        }


                        let desc = arc_single_texture_desc.lock().unwrap();
                        let desc_width = desc.Width;
                        let desc_height = desc.Height;
                        drop(desc);

                        let mut scaler = ffmpeg_next::software::scaling::context::Context::get(
                            ffmpeg_next::format::Pixel::BGRA, // Source pixel format
                            desc_width,
                            desc_height,
                            ffmpeg_next::format::Pixel::YUV420P, // Destination pixel format
                            width,
                            height,
                            ffmpeg_next::software::scaling::Flags::BILINEAR,
                        ).unwrap();

                        let mut src_frame = ffmpeg_next::util::frame::video::Video::new(ffmpeg_next::format::Pixel::BGRA, desc_width, desc_height);


                        let mut dst_frame = ffmpeg_next::util::frame::video::Video::new(ffmpeg_next::format::Pixel::YUV420P, width, height);


                        let bytes_per_pixel = 4;
                        let scanline_bytes = desc_width * bytes_per_pixel;
                        let base_ptr = mapped.pData as *const u8;
                        let base = NonNull::new(base_ptr as *mut u8)
                            .expect("Mapped pData should never be null");
                        let mut frame_buffer = Vec::with_capacity((width * height * 4) as usize);
                        let s_time = Instant::now();
                        for row in 0..desc_height {
                            let offset = row * mapped.RowPitch;
                            // SAFETY: we know each scanline is contiguous memory of length scanline_bytes
                            let slice = unsafe {
                                std::slice::from_raw_parts(base.as_ptr().add(offset as usize), scanline_bytes as usize)
                            };
                            frame_buffer.extend_from_slice(slice);
                        }
                        let src_data = src_frame.data_mut(0);
                        src_data.copy_from_slice(&frame_buffer);

                        scaler.run(&src_frame, &mut dst_frame).unwrap();

                        video_encoder.send_frame(&dst_frame).unwrap();
                        println!("{}", s_time.elapsed().as_millis());

                        let mut packet: codec::packet::Packet = codec::packet::Packet::empty();
                        let _ = video_encoder.receive_packet(&mut packet);

                        println!("{}", i);
                        if let Some(veektor) = packet.data() {
                            println!("{}", veektor.to_vec().len());

                            std::process::exit(69);
                        }

                    }
                    // again, like does it really matter?
                    unsafe {
                        duplication.ReleaseFrame().unwrap();
                    }
                }
            }
        });
    }

    pub fn standard_save(&mut self) {
        println!("start copy out");
        self.ring_buffer_old.copy_out(&self.context, &mut self.standard_out_buffer);
        println!("end copy out");

        println!("start mp4 save");
        VideoData::save_as_mp4(&self.context, &self.standard_out_buffer, self.fps).unwrap();
        println!("end mp4 save");
    }
}