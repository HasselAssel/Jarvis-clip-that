use wasapi::{
    initialize_mta, get_default_device, Device, AudioClient, AudioCaptureClient,
    WaveFormat, SampleType, Direction, StreamMode,
};
use std::{sync::{Arc, Mutex}, thread};
use std::collections::VecDeque;
use std::thread::{JoinHandle, sleep};
use std::time::{Duration, Instant};
use crate::capturer::ring_buffer::{PacketWrapper, RingBuffer};

pub struct AudioCapturer {
    fps: i32,

    audio_encoder: Arc<Mutex<ffmpeg_next::codec::encoder::Audio>>,
    ring_buffer: Arc<Mutex<RingBuffer>>,
}

impl AudioCapturer {
    pub fn new(fps: i32, ring_buffer: Arc<Mutex<RingBuffer>>) -> (Self, Arc<Mutex<ffmpeg_next::codec::encoder::Audio>>) {
        let codec = ffmpeg_next::codec::encoder::find(ffmpeg_next::codec::Id::AAC)
            .ok_or("AAC encoder not found").unwrap();
        let ctx = ffmpeg_next::codec::context::Context::new_with_codec(codec);
        let mut enc = ctx.encoder().audio().unwrap();
        enc.set_rate(44_100);
        enc.set_channel_layout(ffmpeg_next::util::channel_layout::ChannelLayout::STEREO);
        enc.set_format(ffmpeg_next::format::Sample::F32(ffmpeg_next::util::format::sample::Type::Planar));
        enc.set_bit_rate(128_000);
        enc.set_time_base((1, 44_100));

        let _a = enc.open_as(codec).unwrap();
        let audio_encoder = Arc::new(Mutex::new(_a));
        let audio_encoder_return = Arc::clone(&audio_encoder);

        (Self {
            fps,
            audio_encoder,
            ring_buffer,
        },
         audio_encoder_return)
    }

    pub fn start_capturing(self) -> JoinHandle<Result<(), ()>> {
        thread::spawn(move || -> Result<(), ()> {
            let frame_duration: Duration = Duration::from_secs_f64(1.0f64 / (self.fps as f64));
            let mut elapsed: Duration;
            let mut expected_elapsed: Duration;
            let mut start_time: Instant;


            let mut frame_counter = 0;

            initialize_mta().unwrap();

            let device: Device = get_default_device(&Direction::Render).unwrap();
            let mut audio_client: AudioClient = device.get_iaudioclient().unwrap();

            let format = WaveFormat::new(
                32, 32, &SampleType::Float, 44_100, 2, None,
            );
            let mode = StreamMode::EventsShared {
                autoconvert: true,
                buffer_duration_hns: 200_000,
            };
            audio_client.initialize_client(
                &format,
                &Direction::Capture,
                &mode,
            ).unwrap();

            let event_handle = audio_client.set_get_eventhandle().unwrap();
            let capture_client: AudioCaptureClient = audio_client.get_audiocaptureclient().unwrap();

            audio_client.start_stream().unwrap();

            let mut left_buffer = VecDeque::new();
            let mut right_buffer = VecDeque::new();

            let mut zähler = 0;


            loop {
                start_time = Instant::now();
                for i in 0..u32::MAX {
                    elapsed = start_time.elapsed();

                    expected_elapsed = frame_duration.saturating_mul(i);
                    if expected_elapsed > elapsed {
                        sleep(expected_elapsed - elapsed);
                    }
                    zähler += 1;

                    event_handle.wait_for_event(u32::MAX).unwrap();

                    if let Ok(Some(next_packets_frames)) = capture_client.get_next_packet_size() {
                        if next_packets_frames == 0 {
                            continue;
                        }
                        let format = audio_client.get_mixformat().unwrap();
                        let bytes_per_frame = format.wave_fmt.Format.nBlockAlign;
                        let bytes_needed = next_packets_frames * bytes_per_frame as u32;

                        let bytes_per_piped_ffmpeg_frame = ((1024 / 2) * bytes_per_frame) as usize;

                        let mut buffer = vec![0u8; bytes_needed as usize];
                        if let Ok((n, flags)) = capture_client.read_from_device(&mut buffer) {

                            let mut audio_frame = ffmpeg_next::frame::Audio::new(ffmpeg_next::format::Sample::F32(ffmpeg_next::util::format::sample::Type::Planar), /*next_packets_frames*/1024usize, ffmpeg_next::util::channel_layout::ChannelLayout::STEREO);
                            audio_frame.set_rate(/*format.wave_fmt.Format.nSamplesPerSec*/ 44_100);

                            let (left, right): (Vec<u8>, Vec<u8>) = buffer.chunks(8)
                                .fold((Vec::new(), Vec::new()), |(mut left, mut right), chunk| {
                                    if chunk.len() >= 8 {
                                        left.extend_from_slice(&chunk[0..4]);
                                        right.extend_from_slice(&chunk[4..8]);
                                    } else {
                                        println!("Data not divisible by 8");
                                    }
                                    (left, right)
                                });
                            left_buffer.extend(left);
                            right_buffer.extend(right);

                            if left_buffer.len() <  bytes_per_piped_ffmpeg_frame || right_buffer.len() < bytes_per_piped_ffmpeg_frame {
                                continue;
                            }

                            let linesize = unsafe { (*audio_frame.as_ptr()).linesize[0] as usize };
                            let ptr0 = unsafe { (*audio_frame.as_ptr()).extended_data.offset(0).read() };
                            let ptr1 = unsafe { (*audio_frame.as_ptr()).extended_data.offset(1).read() };

                            let left_plane = unsafe { std::slice::from_raw_parts_mut(ptr0, linesize) };
                            let right_plane = unsafe { std::slice::from_raw_parts_mut(ptr1, linesize) };

                            let mut dst_data = audio_frame.data_mut(0);
                            //left_plane.copy_from_slice(&left);
                            let l: Vec<u8> = left_buffer.drain(0..bytes_per_piped_ffmpeg_frame).collect();
                            let r: Vec<u8> = right_buffer.drain(0..bytes_per_piped_ffmpeg_frame).collect();
                            dst_data.copy_from_slice(&l);
                            right_plane.copy_from_slice(&r);

                            audio_frame.set_pts(Some(frame_counter));
                            //audio_frame.set_pts(Some((zähler * 44_100 / self.fps) as i64));
                            frame_counter += next_packets_frames as i64;

                            let mut audio_encoder = self.audio_encoder.lock().unwrap();
                            audio_encoder.send_frame(&audio_frame).unwrap();

                            let mut packet: ffmpeg_next::codec::packet::Packet = ffmpeg_next::codec::packet::Packet::empty();

                            while let Ok(_) = audio_encoder.receive_packet(&mut packet) {
                                let mut ring_buffer = self.ring_buffer.lock().unwrap();
                                ring_buffer.insert(PacketWrapper::new(0, packet.clone()));
                                drop(ring_buffer);
                                packet = ffmpeg_next::codec::packet::Packet::empty();
                            }
                            drop(audio_encoder)
                        }
                    }
                }
            }
            //audio_client.stop_stream().unwrap();
        })
    }
}