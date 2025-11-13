use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use ffmpeg_next::{ChannelLayout, Codec};
use ffmpeg_next::codec::Flags;
use ffmpeg_next::encoder::audio;
use ffmpeg_next::encoder::audio::Encoder;
use ffmpeg_next::format::Sample;
use ffmpeg_next::frame::Audio;
use windows::Win32::Media::Audio::{IAudioCaptureClient, WAVEFORMATEX};

use crate::ring_buffer::traits::PacketRingBuffer;
use crate::types::Result;

pub trait WasapiEncoderCtx {
    fn process_audio<PRB: PacketRingBuffer>(
        &mut self,
        ring_buffer: &Arc<Mutex<PRB>>,
        encoder: &mut Encoder,
        frame: &mut Audio,
        silent_frame: &mut Audio,
        packet_length: u32,
        data: *mut u8,
        qpc_pos: u64,
        start_time: i64,
        frequency: i64,
        format: &WAVEFORMATEX,
        pts_counter: &mut i64,
        audio_buffer: &mut VecDeque<u8>,
        capture_client: &IAudioCaptureClient,
    ) -> Result<()>;
}

pub fn new_audio_encoder_aac(
    mut enc: audio::Audio,
    codec: Codec, rate: i32,
    channel_layout: ChannelLayout,
    sample: Sample,
) -> Result<Encoder> {
    enc.set_rate(rate);
    enc.set_channel_layout(channel_layout);
    enc.set_format(sample);
    enc.set_time_base((1, rate));
    enc.set_flags(Flags::GLOBAL_HEADER);

    let audio_encoder = enc.open_as(codec).unwrap();
    Ok(audio_encoder)
}