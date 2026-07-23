use std::sync::Arc;
use std::sync::Mutex;

use ffmpeg_next::encoder::audio::Encoder;
use ffmpeg_next::util::frame::audio::Audio;

use crate::ring_buffer::traits::PacketRingBuffer;
use crate::types::Result;

pub trait AudioSource {
    fn init(&mut self) -> Result<()>;
    fn await_new_audio(&mut self);
    fn gather_new_audio<PRB: PacketRingBuffer>(
        &mut self,
        ring_buffer: &Arc<Mutex<PRB>>,
        encoder: &mut Encoder,
        frame: &mut Audio,
        silent_frame: &mut Audio,
    ) -> Result<()>;
}
