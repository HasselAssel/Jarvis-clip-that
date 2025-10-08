use std::sync::{Arc, Mutex};
use std::thread;
use std::thread::{JoinHandle, sleep};
use std::time::{Duration, Instant};

use ffmpeg_next::encoder::video::Encoder;
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
    fn start_capturing(mut self: Box<Self>) -> JoinHandle<Result<()>> {
        thread::spawn(move || -> Result<()> {
            self.video_source.init();

            let mut frame = unsafe { ffmpeg_next::frame::video::Video::wrap(*self.av_frame) };

            let frame_duration: Duration = Duration::from_secs_f64(1.0f64 / self.fps);
            let mut elapsed: Duration;
            let mut expected_elapsed: Duration;
            let mut start_time: Instant;

            let mut total_frames_counter = 0;

            loop {
                start_time = Instant::now();

                for i in 0..u32::MAX {
                    elapsed = start_time.elapsed();

                    expected_elapsed = frame_duration.saturating_mul(i);
                    if expected_elapsed > elapsed {
                        sleep(expected_elapsed - elapsed);
                    }

                    let _ = self.video_source.get_frame(&self.av_frame, self.width, self.height);


                    total_frames_counter += 1;

                    frame.set_pts(Some(total_frames_counter));

                    send_frame_and_receive_packets(&self.ring_buffer, &mut self.video_encoder, &frame, 1).unwrap();
                }
            }
        })
    }
}