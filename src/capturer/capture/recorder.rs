use crate::capturer::error::IdkCustomErrorIGuess;
use crate::capturer::ring_buffer::PacketRingBuffer;
use ffmpeg_next::{codec, ChannelLayout, Packet};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

pub struct RecorderCarrier<P: PacketRingBuffer, R: Recorder<P>> {
    recorder: Option<R>,
    pub ring_buffer: Arc<Mutex<P>>,
}

impl<P: PacketRingBuffer, R: Recorder<P>> RecorderCarrier<P, R> {
    pub fn new(ring_buffer: Arc<Mutex<P>>, recorder: R) -> Self {
        Self{
            recorder: Some(recorder),
            ring_buffer,
        }
    }

    pub fn start_capturing(&mut self) -> Result<JoinHandle<Result<(), IdkCustomErrorIGuess>>, IdkCustomErrorIGuess> {
        match self.recorder.take() {
            Some(recorder) => { Ok(recorder.start_capturing()) }
            None => {Err(todo!())}
        }
    }
}


pub trait Recorder<R: PacketRingBuffer> {
    fn start_capturing(self) -> JoinHandle<Result<(), IdkCustomErrorIGuess>>;
    fn send_frame_and_receive_packets(ring_buffer: &Arc<Mutex<R>>, encoder: &mut codec::encoder::Encoder, frame: &ffmpeg_next::Frame, mut duration: i64) {
        let pts = frame.pts().unwrap();
        encoder.send_frame(frame).unwrap();

        let mut packet = Packet::empty();
        let mut ring_buffer = ring_buffer.lock().unwrap();
        while encoder.receive_packet(&mut packet).is_ok() {
            let mut packet_clone = packet.clone();
            packet_clone.set_duration(duration);
            ring_buffer.insert(packet_clone);
            duration = 0;
        }
        drop(ring_buffer);
    }
}

#[derive(Clone)]
pub struct BaseParams {
    pub codec: codec::codec::Codec,
    pub bit_rate: usize,
    pub max_bit_rate: usize,
    pub flags: codec::flag::Flags,
    pub rate: i32, // "fps"
}

#[derive(Clone)]
pub struct VideoParams {
    pub base_params: BaseParams,

    pub out_width: u32,
    pub out_height: u32,
    pub format: ffmpeg_next::util::format::pixel::Pixel,
}

#[derive(Clone)]
pub struct AudioParams {
    pub base_params: BaseParams,

    pub channel_layout: ChannelLayout,
    pub format: ffmpeg_next::util::format::sample::Sample,

}