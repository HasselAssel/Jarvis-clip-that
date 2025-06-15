use std::ptr::null_mut;
use std::sync::{Arc, Mutex};
use std::thread;
use std::thread::{JoinHandle, sleep};
use std::time::{Duration, Instant};

use ffmpeg_next::encoder::video::Encoder;
use ffmpeg_next::sys::{av_buffer_create, AVFrame};
use windows::Win32::Graphics::Direct3D11::{ID3D11Device, ID3D11DeviceContext, ID3D11Texture2D};
use windows::Win32::Graphics::Dxgi::{DXGI_OUTDUPL_FRAME_INFO, IDXGIOutputDuplication, IDXGIResource};
use windows_core::Interface;

use crate::recorder::parameters::VideoParams;
use crate::recorder::traits::Recorder;
use crate::ring_buffer::traits::PacketRingBuffer;
use crate::types::Result;

pub struct VideoCapturer<P: PacketRingBuffer> {
    ring_buffer: Arc<Mutex<P>>,
    video_encoder: Encoder,
    video_params: VideoParams,
    device: ID3D11Device,
    context: ID3D11DeviceContext,
    duplication: IDXGIOutputDuplication,
    av_frame: *mut AVFrame,
}

impl<P: PacketRingBuffer> VideoCapturer<P> {
    pub fn new(ring_buffer: Arc<Mutex<P>>, video_encoder: Encoder, video_params: VideoParams, (device, context, duplication): (ID3D11Device, ID3D11DeviceContext, IDXGIOutputDuplication), av_frame: *mut AVFrame) -> Self {
        Self {
            ring_buffer,
            video_encoder,
            video_params,
            device,
            context,
            duplication,
            av_frame,
        }
    }
}

impl<P: PacketRingBuffer> Recorder<P> for VideoCapturer<P> {
    fn start_capturing(mut self) -> JoinHandle<Result<()>> {
        thread::spawn(move || -> Result<()> {
            let mut resource: Option<IDXGIResource>;
            let mut hr: windows::core::Result<()>;
            let mut frame_info: DXGI_OUTDUPL_FRAME_INFO = DXGI_OUTDUPL_FRAME_INFO::default();
            let mut tex: ID3D11Texture2D;
            let mut nv12_tex: ID3D11Texture2D;
            let mut av_frame: *mut AVFrame = null_mut();
            let mut frame: ffmpeg_next::frame::Video = ffmpeg_next::frame::Video::new(ffmpeg_next::format::Pixel::NV12, self.video_params.out_width, self.video_params.out_height);

            let frame_duration: Duration = Duration::from_secs_f64(1.0f64 / (self.video_params.base_params.rate as f64));
            let mut elapsed: Duration;
            let mut expected_elapsed: Duration;
            let mut start_time: Instant;

            let mut packet_sent_frames_counter = 0;
            let mut total_frames_counter = 0;

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
                                    self.convert_rgba_to_nv12(&tex).unwrap()
                                };

                                // TRUST THE PROCESS
                                let texture_buffer = unsafe {
                                    av_buffer_create(
                                        nv12_tex.as_raw() as _,
                                        size_of::<*mut ID3D11Texture2D>(),
                                        Some(Self::buffer_free),
                                        null_mut(),
                                        0,
                                    )
                                };
                                unsafe {
                                    (*av_frame).data[0] = nv12_tex.as_raw() as _;
                                    (*av_frame).buf[0] = texture_buffer;
                                }
                            };
                            Ok(())
                        };

                        // how can releasing a frame not be safe, right?
                        unsafe { self.duplication.ReleaseFrame().unwrap(); }
                        _exists_new_frame
                    };

                    // TODO: Fix First Frame always being Green (for some reason the first duplication.AcquireNextFrame call generates no IDXGIResource)


                    if let Ok(_) = exists_new_frame {
                        frame = unsafe { ffmpeg_next::frame::Video::wrap(av_frame) };
                    }
                    total_frames_counter += 1;
                    packet_sent_frames_counter += 1;

                    frame.set_pts(Some(total_frames_counter));

                    Self::send_frame_and_receive_packets(&self.ring_buffer, &mut self.video_encoder, &frame, 1).unwrap();
                }
            }
        })
    }
}