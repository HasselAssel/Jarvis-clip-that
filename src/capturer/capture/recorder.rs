use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use ffmpeg_next::{codec, Packet, rational};
use ffmpeg_next::codec::Id;
use crate::capturer::error::IdkCustomErrorIGuess;
use crate::capturer::ring_buffer::PacketRingBuffer;

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
    fn send_frame_and_receive_packets(ring_buffer: &Arc<Mutex<R>>, encoder: &mut ffmpeg_next::codec::encoder::Encoder, frame: &ffmpeg_next::Frame) {
        encoder.send_frame(frame).unwrap();

        let mut packet = Packet::empty();
        let mut ring_buffer = ring_buffer.lock().unwrap();
        while encoder.receive_packet(&mut packet).is_ok() {
            ring_buffer.insert(packet.clone());
        }
        drop(ring_buffer);
    }
}

struct BaseParams {

}

pub struct VideoParams {
    pub base_params: BaseParams,
    pub parameters: codec::Parameters,
    pub time_base: rational::Rational,
    pub codec: Id,
}