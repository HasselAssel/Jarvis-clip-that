use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::sync::mpsc::Receiver;
use std::time::Duration;

use atomic_float::AtomicF32;
use ffmpeg_next::format::Sample;
use ffmpeg_next::frame::Audio;
use rodio::Source;

pub struct LiveSource {
    pub receiver: Receiver<f32>,
    pub sample_rate: u32,
    pub channels: u16,
    pub volume: Arc<AtomicF32>,
}

impl Iterator for LiveSource {
    type Item = f32;

    fn next(&mut self) -> Option<Self::Item> {
        self.receiver.recv().ok().map(|s| s * &self.volume.load(Ordering::SeqCst))
    }
}

impl Source for LiveSource {
    fn current_frame_len(&self) -> Option<usize> { None }
    fn channels(&self) -> u16 { self.channels }
    fn sample_rate(&self) -> u32 { self.sample_rate }
    fn total_duration(&self) -> Option<Duration> { None }
}


pub fn frame_to_interleaved_f32(frame: &Audio) -> Vec<f32> {
    let nb_samples = frame.samples();
    let nb_channels = frame.channels() as usize;
    let plane = frame.data(0);
    let mut out = Vec::with_capacity(nb_samples * nb_channels);

    match frame.format() {
        Sample::U8(_) => {
            for &b in plane.iter() {
                // convert [0,255] -> [-1.0,1.0]
                out.push(b as f32 / 127.5 - 1.0);
            }
        }
        Sample::I16(_) => {
            for chunk in plane.chunks_exact(2) {
                let s = i16::from_le_bytes([chunk[0], chunk[1]]);
                out.push(s as f32 / 32768.0);
            }
        }
        Sample::I32(_) => {
            for chunk in plane.chunks_exact(4) {
                let s = i32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
                out.push(s as f32 / 2_147_483_648.0);
            }
        }
        Sample::F32(_) => {
            for chunk in plane.chunks_exact(4) {
                out.push(f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]));
            }
        }
        Sample::F64(_) => {
            for chunk in plane.chunks_exact(8) {
                let s = f64::from_le_bytes([
                    chunk[0], chunk[1], chunk[2], chunk[3],
                    chunk[4], chunk[5], chunk[6], chunk[7],
                ]);
                out.push(s as f32);
            }
        }
        _ => panic!("Unsupported sample format {:?}", frame.format()),
    }

    out
}