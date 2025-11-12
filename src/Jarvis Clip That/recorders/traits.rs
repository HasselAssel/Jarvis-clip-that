use std::sync::{Arc, Mutex};
use std::sync::atomic::AtomicBool;
use ffmpeg_next::codec;
use crate::ring_buffer::traits::PacketRingBuffer;
use crate::types::{Packet, Result, RecorderJoinHandle};

pub trait TRecorder<PRB: PacketRingBuffer> {
    fn start_capturing(self: Box<Self>, stop_capturing_callback: Option<Arc<AtomicBool>>) -> RecorderJoinHandle;
}

pub fn send_frame_and_receive_packets<PRB: PacketRingBuffer>(ring_buffer: &Arc<Mutex<PRB>>, encoder: &mut codec::encoder::Encoder, frame: &ffmpeg_next::Frame, mut duration: i64) -> Result<()> {
    encoder.send_frame(frame)?;

    let mut packet = Packet::empty();
    let mut ring_buffer = ring_buffer.lock().unwrap();
    while encoder.receive_packet(&mut packet).is_ok() {
        let mut packet_clone = packet.clone();
        packet_clone.set_duration(duration);
        ring_buffer.insert(packet_clone);
        duration = 0;
    }
    drop(ring_buffer);
    Ok(())
}