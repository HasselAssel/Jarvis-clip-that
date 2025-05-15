use std::ptr::{NonNull, null_mut};
use std::sync::{Arc, Mutex};
use std::{ptr, thread};
use std::mem::ManuallyDrop;
use std::thread::{JoinHandle, sleep};
use std::time::{Duration, Instant};
use ffmpeg_next::{Frame, Packet};
use ffmpeg_next::frame::Video;
use ffmpeg_next::sys::{av_buffer_create, av_buffer_ref, av_buffer_unref, av_frame_alloc, av_hwdevice_ctx_alloc, av_hwdevice_ctx_init, av_hwframe_ctx_alloc, av_hwframe_ctx_init, av_hwframe_get_buffer, AVBufferRef, AVFrame, AVHWDeviceType};
use ffmpeg_next::sys::AVPixelFormat::{AV_PIX_FMT_D3D11, AV_PIX_FMT_NV12};

use windows::core::{Result, Interface, s};
use windows::Win32::Foundation::{HMODULE, TRUE};
use windows::Win32::Graphics::Direct3D::{D3D_DRIVER_TYPE_HARDWARE, D3D_FEATURE_LEVEL_10_0, D3D_FEATURE_LEVEL_11_0};
use windows::Win32::Graphics::Direct3D11::{D3D11_BIND_RENDER_TARGET, D3D11_BIND_SHADER_RESOURCE, D3D11_BIND_VIDEO_ENCODER, D3D11_CPU_ACCESS_READ, D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_MAP_READ, D3D11_MAPPED_SUBRESOURCE, D3D11_SDK_VERSION, D3D11_TEX2D_VPIV, D3D11_TEX2D_VPOV, D3D11_TEXTURE2D_DESC, D3D11_USAGE_DEFAULT, D3D11_USAGE_STAGING, D3D11_VIDEO_FRAME_FORMAT_PROGRESSIVE, D3D11_VIDEO_PROCESSOR_CONTENT_DESC, D3D11_VIDEO_PROCESSOR_INPUT_VIEW_DESC, D3D11_VIDEO_PROCESSOR_INPUT_VIEW_DESC_0, D3D11_VIDEO_PROCESSOR_OUTPUT_VIEW_DESC, D3D11_VIDEO_PROCESSOR_OUTPUT_VIEW_DESC_0, D3D11_VIDEO_PROCESSOR_STREAM, D3D11_VIDEO_USAGE_PLAYBACK_NORMAL, D3D11_VPIV_DIMENSION_TEXTURE2D, D3D11_VPOV_DIMENSION_TEXTURE2D, D3D11CreateDevice, ID3D11Device, ID3D11DeviceContext, ID3D11Resource, ID3D11Texture2D, ID3D11VideoContext, ID3D11VideoDevice, ID3D11VideoProcessor, ID3D11VideoProcessorEnumerator, ID3D11VideoProcessorInputView, ID3D11VideoProcessorOutputView};
use windows::Win32::Graphics::Dxgi::{DXGI_OUTDUPL_DESC, DXGI_OUTDUPL_FRAME_INFO, IDXGIAdapter, IDXGIDevice, IDXGIOutput, IDXGIOutput1, IDXGIOutputDuplication, IDXGIResource};
use windows::Win32::Graphics::Dxgi::Common::{DXGI_CPU_ACCESS_NONE, DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_FORMAT_NV12, DXGI_RATIONAL, DXGI_SAMPLE_DESC};

use crate::capturer::ring_buffer::{RingBuffer, PacketWrapper};

#[repr(C)]
struct AVD3D11FrameDescriptor {
    texture: *mut ID3D11Texture2D,
    index: isize,
}

impl AVD3D11FrameDescriptor {
    // # Safety
    // Must point at a valid AVD3D11FrameDescriptor allocated by FFmpeg.
    unsafe fn from_buf(buf: *mut AVBufferRef) -> *mut ID3D11Texture2D {
        let desc_ptr = (*buf).data as *mut AVD3D11FrameDescriptor;
        (*desc_ptr).texture
    }
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

    device: ID3D11Device,
    hw_frame_ctx: usize,
}

impl Capturer {
    pub fn _new(fps: i32, out_width: u32, out_height: u32, ring_buffer: Arc<Mutex<RingBuffer>>) -> (Self, Arc<Mutex<ffmpeg_next::codec::encoder::Video>>) {
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

            device,
            hw_frame_ctx: unsafe { 0 as _ },
        },
         video_encoder_return)
    }

    pub fn _start_capturing(self) -> JoinHandle<Result<()>> {
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

            let mut scaler: ffmpeg_next::software::scaling::context::Context = ffmpeg_next::software::scaling::context::Context::get(
                ffmpeg_next::format::Pixel::BGRA, // Source pixel format
                self.single_texture_desc.Width,
                self.single_texture_desc.Height,
                ffmpeg_next::format::Pixel::YUV420P, // Destination pixel format
                self.out_width,
                self.out_height,
                ffmpeg_next::software::scaling::Flags::BILINEAR,
            ).unwrap();
            let mut frame_buffer: Vec<u8> = Vec::with_capacity((self.single_texture_desc.Width * self.single_texture_desc.Height * 4) as usize);
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

                                let base_ptr = mapped.pData as *const u8;
                                let base = NonNull::new(base_ptr as *mut u8)
                                    .expect("Mapped pData should never be null");

                                frame_buffer.clear();
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
                        //packet = ffmpeg_next::codec::packet::Packet::empty();
                        println!("RECEIVER");
                    }
                    drop(ring_buffer);
                    drop(video_encoder);

                    eprintln!("Encode+I/O: {:?}", t_encode.elapsed());
                }
            }
        })
    }

    pub fn new(fps: i32, out_width: u32, out_height: u32, ring_buffer: Arc<Mutex<RingBuffer>>) -> (Self, Arc<Mutex<ffmpeg_next::codec::encoder::Video>>) {
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

        #[repr(C)]
        pub struct AVD3D11VADeviceContext {
            pub device: *mut ID3D11Device,
            pub device_context: *mut ID3D11DeviceContext,
            pub video_device: *mut ID3D11VideoDevice,
            pub video_context: *mut ID3D11VideoContext,
        }

        let codec = ffmpeg_next::codec::encoder::find_by_name("hevc_amf")
            .ok_or(ffmpeg_next::Error::EncoderNotFound).unwrap();

        let mut ctx = ffmpeg_next::codec::context::Context::new_with_codec(codec);

        let mut hw_device_ctx = unsafe { av_hwdevice_ctx_alloc(AVHWDeviceType::AV_HWDEVICE_TYPE_D3D11VA) };
        if hw_device_ctx.is_null() {
            panic!("Failed to allocate HW device context");
        }

        let hwctx = unsafe {
            let device_ctx = (*hw_device_ctx).data as *mut ffmpeg_next::sys::AVHWDeviceContext;
            (*device_ctx).hwctx as *mut AVD3D11VADeviceContext
        };

        unsafe {
            (*hwctx).device = device.as_raw() as *mut _;
        }


        // Initialize the context
        let ret = unsafe { av_hwdevice_ctx_init(hw_device_ctx) };
        unsafe {
            if ret < 0 as _ {
                println!("TEST 0.75");
                av_buffer_unref(&mut hw_device_ctx);
                panic!("Failed to initialize HW device context");
            }
        }


        let mut hw_frame_ctx: *mut ffmpeg_next::sys::AVBufferRef = unsafe { ffmpeg_next::sys::av_hwframe_ctx_alloc(hw_device_ctx) };
        if hw_frame_ctx.is_null() { panic!("alloc failed"); }

        let frames_ctx = unsafe { &mut *((*hw_frame_ctx).data as *mut ffmpeg_next::sys::AVHWFramesContext) };
        frames_ctx.format = ffmpeg_next::sys::AVPixelFormat::AV_PIX_FMT_D3D11;
        frames_ctx.sw_format = ffmpeg_next::sys::AVPixelFormat::AV_PIX_FMT_NV12;
        frames_ctx.width = out_width as i32;
        frames_ctx.height = out_height as i32;
        frames_ctx.initial_pool_size = 0;

        let ret = unsafe { av_hwframe_ctx_init(hw_frame_ctx) };
        if ret < 0 { panic!("init failed"); }


        let mut enc = ctx.encoder().video().unwrap();

        let raw_ctx = unsafe { enc.as_mut_ptr() };

        unsafe {
            (*raw_ctx).hw_device_ctx = ffmpeg_next::sys::av_buffer_ref(hw_device_ctx);
            (*raw_ctx).hw_frames_ctx = ffmpeg_next::sys::av_buffer_ref(hw_frame_ctx);
        }

        enc.set_width(out_width);
        enc.set_height(out_height);
        enc.set_format(ffmpeg_next::format::Pixel::D3D11);
        enc.set_time_base((1, fps));
        enc.set_frame_rate(Some((fps, 1)));
        enc.set_bit_rate(8_000_000);
        enc.set_max_bit_rate(10_000_000);

        enc.set_flags(ffmpeg_next::codec::Flags::GLOBAL_HEADER); // Extradata is generated
        enc.set_gop(fps as u32); // Keyframe interval (1 second)

        let _v = enc.open_as(codec).unwrap();
        let video_encoder = Arc::new(Mutex::new(_v));
        let video_encoder_return = Arc::clone(&video_encoder);


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
            //Format: DXGI_FORMAT_NV12,
            Format: DXGI_FORMAT_B8G8R8A8_UNORM,
            SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
            Usage: D3D11_USAGE_DEFAULT,
            BindFlags: (D3D11_BIND_SHADER_RESOURCE | D3D11_BIND_RENDER_TARGET).0 as u32,
            CPUAccessFlags: 0,
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

            device,
            hw_frame_ctx: hw_frame_ctx as usize,
        },
         video_encoder_return)
    }

    pub fn start_capturing(self) -> JoinHandle<Result<()>> {
        thread::spawn(move || -> Result<()> {
            let mut resource: Option<IDXGIResource>;
            let mut hr: Result<()>;
            let mut frame_info: DXGI_OUTDUPL_FRAME_INFO = DXGI_OUTDUPL_FRAME_INFO::default();
            let mut tex: ID3D11Texture2D;
            let mut nv12_tex: ID3D11Texture2D;

            let mut av_frame: *mut AVFrame = null_mut();
            let mut frame: Video;
            let mut packet: Packet = Packet::empty();

            let frame_duration: Duration = Duration::from_secs_f64(1.0f64 / (self.fps as f64));
            let mut elapsed: Duration;
            let mut expected_elapsed: Duration;
            let mut start_time: Instant;

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

                    let exists_new_frame = 'r: {
                        resource = None;
                        // surely this is safe!
                        unsafe { hr = self.duplication.AcquireNextFrame(0, &mut frame_info, &mut resource); }

                        if hr.is_err() {
                            break 'r Err(format!("IDXGIOutputDuplication::AcquireNextFrame returned Error value: {:?}", hr));
                        }

                        let _exists_new_frame = 'rr: {
                            if let Some(dxgi_resource) = resource {
                                if frame_info.AccumulatedFrames == 0 {
                                    break 'rr Err("DXGI_OUTDUPL_FRAME_INFO.AccumulatedFrames is 0".to_string());
                                }
                                tex = dxgi_resource.cast().unwrap();

                                nv12_tex = unsafe {
                                    self.convert_rgba_to_nv12(
                                        &tex,
                                        self.out_width,
                                        self.out_height,
                                    ).unwrap()
                                };

                                // TRUST THE PROCESS
                                let texture_buffer = unsafe {
                                    av_buffer_create(
                                        nv12_tex.as_raw() as _,
                                        std::mem::size_of::<*mut ID3D11Texture2D>(),
                                        Some(Self::buffer_free),
                                        ptr::null_mut(),
                                        0,
                                    )
                                };

                                unsafe {
                                    av_frame = av_frame_alloc();
                                    (*av_frame).format = AV_PIX_FMT_D3D11 as i32;
                                    (*av_frame).width = self.out_width as i32;
                                    (*av_frame).height = self.out_height as i32;
                                    (*av_frame).hw_frames_ctx = av_buffer_ref(self.hw_frame_ctx as *mut AVBufferRef);
                                }
                                let ret = unsafe {
                                    av_hwframe_get_buffer(self.hw_frame_ctx as *mut AVBufferRef, av_frame, 0)
                                };
                                if ret < 0 || av_frame.is_null() {
                                    break 'rr Err(format!("av_hwframe_get_buffer failed: {}", ret))
                                }

                                unsafe {
                                    (*av_frame).format = AV_PIX_FMT_D3D11 as i32;
                                    (*av_frame).width = self.out_width as _;
                                    (*av_frame).height = self.out_height as _;
                                    (*av_frame).data[0] = nv12_tex.as_raw() as _;
                                    (*av_frame).buf[0] = texture_buffer;
                                    (*av_frame).hw_frames_ctx = av_buffer_ref(self.hw_frame_ctx as _);
                                }
                            };
                            Ok(())
                        };

                        // how can releasing a frame not be safe, right?
                        unsafe { self.duplication.ReleaseFrame().unwrap(); }
                        _exists_new_frame
                    };

                    // Todo: Fix that when capturing right when the screen is frozen, the frozen time is not saved cause no frame with pts at capture time exists

                    if let Ok(_) = exists_new_frame {
                        frame = unsafe { Video::wrap(av_frame) };
                        frame.set_pts(Some(frame_counter));

                        let mut video_encoder = self.video_encoder.lock().unwrap();
                        video_encoder.send_frame(&frame).unwrap();

                        let mut ring_buffer = self.ring_buffer.lock().unwrap();
                        while let Ok(_) = video_encoder.receive_packet(&mut packet) {
                            ring_buffer.insert(PacketWrapper::new(piped_frames, packet.clone()));
                            piped_frames = 0;
                        }
                        drop(ring_buffer);
                        drop(video_encoder);
                    }
                    frame_counter += 1;
                    piped_frames += 1;
                }
            }
        })
    }

    pub unsafe fn convert_rgba_to_nv12(&self,
                                       tex_rgba: &ID3D11Texture2D,
                                       width: u32,
                                       height: u32,
    ) -> Result<ID3D11Texture2D> {
        // 1) QI for ID3D11VideoDevice
        let video_dev: ID3D11VideoDevice = self.device.cast().unwrap();
        let video_ctx: ID3D11VideoContext = self.context.cast().unwrap();

        // 2) Describe & create the VideoProcessorEnumerator
        let vp_desc = D3D11_VIDEO_PROCESSOR_CONTENT_DESC {
            InputFrameFormat: D3D11_VIDEO_FRAME_FORMAT_PROGRESSIVE,
            InputFrameRate: DXGI_RATIONAL { Numerator: 30, Denominator: 1 },
            InputWidth: width,
            InputHeight: height,
            OutputFrameRate: DXGI_RATIONAL { Numerator: 30, Denominator: 1 },
            OutputWidth: width,
            OutputHeight: height,
            Usage: windows::Win32::Graphics::Direct3D11::D3D11_VIDEO_USAGE_OPTIMAL_SPEED,
        };
        let vp_enum: ID3D11VideoProcessorEnumerator = video_dev.CreateVideoProcessorEnumerator(&vp_desc).unwrap();  // :contentReference[oaicite:5]{index=5}


        //verify
        let _2 = vp_enum.CheckVideoProcessorFormat(DXGI_FORMAT_B8G8R8A8_UNORM).unwrap();
        println!("RESULT WAS: {}", _2);

        // 3) Create the VideoProcessor itself
        let mut vp: ID3D11VideoProcessor = video_dev.CreateVideoProcessor(&vp_enum, 0).unwrap();             // :contentReference[oaicite:6]{index=6}

        // 4) Make the NV12 output texture
        let nv12_desc = D3D11_TEXTURE2D_DESC {
            Width: width,
            Height: height,
            MipLevels: 1,
            ArraySize: 1,
            Format: DXGI_FORMAT_NV12,
            SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
            Usage: D3D11_USAGE_DEFAULT,
            BindFlags: (D3D11_BIND_SHADER_RESOURCE.0 | D3D11_BIND_RENDER_TARGET.0) as u32,
            CPUAccessFlags: 0,
            MiscFlags: 0,
        };
        let mut tex_nv12 = None;
        self.device.CreateTexture2D(&nv12_desc, None, Some(&mut tex_nv12)).unwrap();
        let tex_nv12 = tex_nv12.unwrap();

        // 5) Create processor‐input view for RGBA texture
        let in_view_desc = D3D11_VIDEO_PROCESSOR_INPUT_VIEW_DESC {
            FourCC: 0,
            ViewDimension: D3D11_VPIV_DIMENSION_TEXTURE2D,
            Anonymous: D3D11_VIDEO_PROCESSOR_INPUT_VIEW_DESC_0 {
                Texture2D: D3D11_TEX2D_VPIV {
                    MipSlice: 0,
                    ArraySlice: 0,
                },
            },
        };
        let mut input_view: Option<ID3D11VideoProcessorInputView> = None;
        video_dev.CreateVideoProcessorInputView(
            tex_rgba,
            &vp_enum,
            &in_view_desc,
            Some(&mut input_view),
        ).unwrap();                                                                // :contentReference[oaicite:7]{index=7}
        let input_view = input_view.unwrap();

        // 6) Create processor‐output view for NV12
        let out_view_desc = D3D11_VIDEO_PROCESSOR_OUTPUT_VIEW_DESC {
            ViewDimension: D3D11_VPOV_DIMENSION_TEXTURE2D,
            Anonymous: D3D11_VIDEO_PROCESSOR_OUTPUT_VIEW_DESC_0 {
                Texture2D: D3D11_TEX2D_VPOV {
                    MipSlice: 0,
                },
            },
        };
        let mut output_view: Option<ID3D11VideoProcessorOutputView> = None;
        video_dev.CreateVideoProcessorOutputView(
            &tex_nv12,
            &vp_enum,
            &out_view_desc,
            Some(&mut output_view),
        ).unwrap();                                                                // :contentReference[oaicite:8]{index=8}
        let output_view = output_view.unwrap();

        // 7) Execute the GPU blit
        let _idk = ManuallyDrop::new(Some(input_view.clone()));
        let mut stream = windows::Win32::Graphics::Direct3D11::D3D11_VIDEO_PROCESSOR_STREAM {
            Enable: windows::Win32::Foundation::TRUE,
            pInputSurface: _idk,
            ..Default::default()
        };
        video_ctx.VideoProcessorBlt(&vp, &output_view, 0, &[stream]).unwrap();            // :contentReference[oaicite:9]{index=9}

        // Return the new NV12 texture & processor
        Ok(tex_nv12)
    }

    unsafe extern "C" fn buffer_free(_opaque: *mut std::ffi::c_void, data: *mut u8) {

    }
}