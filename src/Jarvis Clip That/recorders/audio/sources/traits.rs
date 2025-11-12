use std::sync::{Arc, Mutex};
use ffmpeg_next::util::frame::audio::Audio;
use ffmpeg_next::encoder::audio::Encoder;
use crate::ring_buffer::traits::PacketRingBuffer;
use crate::types::Result;

pub trait AudioSource {
    fn init(&mut self);
    fn await_new_audio(&mut self);
    fn gather_new_audio<PRB: PacketRingBuffer>(&mut self, ring_buffer: &Arc<Mutex<PRB>>, encoder: &mut Encoder, frame: &mut Audio, silent_frame: &mut Audio) -> Result<()>;
}