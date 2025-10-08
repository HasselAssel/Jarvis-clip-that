use std::thread;
use std::time::Duration;
use crate::recorders::audio::sources::enums::{AudioCodec, AudioSourceType};
use crate::recorders::recorder::{create_audio_recorder, create_video_recorder};
use crate::recorders::save::saver::Saver;
use crate::recorders::video::sources::enums::{VideoCodec, VideoSourceType};
use crate::ring_buffer::packet_handlers::KeyFrameStartPacketWrapper;
use crate::ring_buffer::ring_buffer::RingBuffer;
use crate::types::Packet;

mod error;
mod types;
mod wrappers;
mod ring_buffer;
mod recorders;

fn main() {
    type VideoPacketRingBufferType = RingBuffer<KeyFrameStartPacketWrapper>;
    type AudioPacketRingBufferType = RingBuffer<Packet>;

    let video_source_type = VideoSourceType::D3d11;
    let video_codec = VideoCodec::Amf;

    let audio_source_type = AudioSourceType::WasApi;
    let audio_codec = AudioCodec::AAC;

    let seconds = 20;
    let fps = 30;

    let mut video_recorder = create_video_recorder::<VideoPacketRingBufferType>(video_source_type, video_codec, seconds, 2560, 1440, fps).unwrap();
    let mut audio_recorder = create_audio_recorder::<AudioPacketRingBufferType>(audio_source_type, audio_codec, seconds).unwrap();
    let saver = Saver::new(video_recorder.parameters.clone(), audio_recorder.parameters.clone(), "out", "Chat Clip That", ".mp4");

    video_recorder.start_recording();
    audio_recorder.start_recording();

    thread::sleep(Duration::from_secs(25));

    //saver.standard_save_to_discTEST(&video_recorder.ring_buffer, None).unwrap();
    saver.standard_save_to_disc(&video_recorder.ring_buffer, &audio_recorder.ring_buffer, None).unwrap()
}