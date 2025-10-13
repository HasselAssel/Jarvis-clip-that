use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::thread::{JoinHandle, sleep};
use std::time::{Duration, Instant};

use ffmpeg_next::encoder::video::Encoder;
use ffmpeg_next::util::frame::video::Video;
use ffmpeg_next::sys::AVFrame;

use crate::recorders::traits::{send_frame_and_receive_packets, TRecorder};
use crate::recorders::video::sources::traits::VideoSource;
use crate::ring_buffer::traits::PacketRingBuffer;
use crate::types::Result;
use crate::wrappers::MaybeSafeFFIPtrWrapper;

pub struct VideoRecorder<PRB: PacketRingBuffer + 'static, VS: VideoSource + Send + 'static> {
    ring_buffer: Arc<Mutex<PRB>>,
    video_source: VS,

    width: u32,
    height: u32,
    fps: f64,

    video_encoder: Encoder,
    av_frame: MaybeSafeFFIPtrWrapper<AVFrame>,
}

impl<PRB: PacketRingBuffer, VS: VideoSource + Send> VideoRecorder<PRB, VS> {
    pub fn new(ring_buffer: Arc<Mutex<PRB>>, video_source: VS, video_encoder: Encoder, av_frame: MaybeSafeFFIPtrWrapper<AVFrame>, width: u32, height: u32, fps: f64) -> Self {
        Self {
            ring_buffer,
            video_source,

            width,
            height,
            fps,

            video_encoder,
            av_frame,
        }
    }
}

impl<PRB: PacketRingBuffer, VS: VideoSource + Send> TRecorder<PRB> for VideoRecorder<PRB, VS> {
    fn start_capturing(mut self: Box<Self>, stop_capturing_callback: Option<Arc<AtomicBool>>) -> JoinHandle<Result<()>> {
        fn help<PRB: PacketRingBuffer, VS: VideoSource + Send>(mut selbst: &mut Box<VideoRecorder<PRB, VS>>, frame: &mut Video, frame_duration: &Duration, mut elapsed: &mut Duration, expected_elapsed: &mut Duration, mut start_time: &mut Instant, mut total_frames_counter: &mut i64) {
            *start_time = Instant::now();

            for i in 0..u32::MAX {
                *elapsed = start_time.elapsed();

                *expected_elapsed = frame_duration.saturating_mul(i);
                if expected_elapsed > elapsed {
                    sleep(*expected_elapsed - *elapsed);
                }

                let _ = selbst.video_source.get_frame(&selbst.av_frame, selbst.width, selbst.height);


                *total_frames_counter += 1;

                frame.set_pts(Some(*total_frames_counter));

                send_frame_and_receive_packets(&selbst.ring_buffer, &mut selbst.video_encoder, &frame, 1).unwrap();
            }
        }

        thread::spawn(move || -> Result<()> {
            self.video_source.init();

            let mut frame = unsafe { Video::wrap(*self.av_frame) };

            let frame_duration: Duration = Duration::from_secs_f64(1.0f64 / self.fps);
            let mut elapsed: Duration = Default::default();
            let mut expected_elapsed: Duration = Default::default();
            let mut start_time: Instant = Instant::now();

            let mut total_frames_counter = 0;

            if let Some(stop_capturing_callback) = stop_capturing_callback {
                while stop_capturing_callback.load(Ordering::Relaxed) {
                    help(&mut self, &mut frame, &frame_duration, &mut elapsed, &mut expected_elapsed, &mut start_time, &mut total_frames_counter);
                }
                Ok(())
            } else {
                loop {
                    help(&mut self, &mut frame, &frame_duration, &mut elapsed, &mut expected_elapsed, &mut start_time, &mut total_frames_counter);
                }
                Ok(())
            }
        })
    }
}