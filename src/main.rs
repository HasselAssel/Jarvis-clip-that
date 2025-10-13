use std::thread;
use std::time::Duration;
use rand::TryRngCore;
use crate::recorders::audio::sources::enums::{AudioCodec, AudioSourceType};
use crate::recorders::audio::sources::wasapi::source::AudioProcessWatcher;
use crate::recorders::recorder::{create_audio_recorder, create_video_recorder};
use crate::recorders::save::_saver::Saver;
use crate::recorders::save::saver::SaverEnv;
use crate::recorders::video::sources::enums::{VideoCodec, VideoSourceType};
use crate::ring_buffer::packet_handlers::KeyFrameStartPacketWrapper;
use crate::ring_buffer::ring_buffer::RingBuffer;
use crate::types::Packet;

mod error;
mod types;
mod wrappers;
mod ring_buffer;
mod recorders;

#[tokio::main]
async fn main() {
    main_sync().await
}


async fn main_sync() {
    /*let pid = loop {
        let mut input = String::new();
        println!("GIMME A PID:");
        std::io::stdin().read_line(&mut input).expect("Failed!");
        match input.trim().parse::<u32>() {
            Ok(n) => break n,
            Err(_) => println!("Invalid input, please enter a valid number."),
        }
    };*/

    type VideoPacketRingBufferType = RingBuffer<KeyFrameStartPacketWrapper>;
    type AudioPacketRingBufferType = RingBuffer<Packet>;

    let video_source_type = VideoSourceType::D3d11 { monitor_id: 0 };
    let video_codec = VideoCodec::Amf;

    //let audio_source_type = AudioSourceType::WasApiProcess { process_id: pid, include_tree: true };
    //let audio_source_type = AudioSourceType::WasApiSys;
    let audio_codec = AudioCodec::AAC;


    let seconds = 8;
    let fps = 30;

    let mut video_recorder = create_video_recorder::<VideoPacketRingBufferType>(&video_source_type, &video_codec, seconds, 2560, 1440, fps).unwrap();
    //let mut audio_recorder = create_audio_recorder::<AudioPacketRingBufferType>(&audio_source_type, &audio_codec, seconds).unwrap();
    let mut audio_recorder = AudioProcessWatcher::<AudioPacketRingBufferType>::new(audio_codec, true, seconds);

    let saver = Saver::new(video_recorder.parameters.clone(), video_recorder.parameters.clone(), "out", "Chat Clip That", ".mp4");
    let save_env = SaverEnv::new("out", "Chat Clip That");

    video_recorder.start_recording(None);
    audio_recorder.start_recording(None).await;

    tokio::time::sleep(Duration::from_secs_f64(8.1)).await;


    let mut save = save_env.new_save::<String>(None);

    let _ = save.add_stream(&video_recorder.ring_buffer, &video_recorder.parameters, true).unwrap();
    //let _ = save.add_stream(&audio_recorder.ring_buffer, &audio_recorder.parameters, false).unwrap();

    for (p_id, (recorder, _, _)) in audio_recorder.audio_recorders.lock().await.iter() {
        println!("stream added for: {}", p_id);
        let _ = save.add_stream(&recorder.ring_buffer, &recorder.parameters, false).unwrap();
    }

    let _ = save.finalize_and_save().unwrap();

    //saver.standard_save_to_discTEST(&video_recorder.ring_buffer, None).unwrap();
    //saver.standard_save_to_discT(&video_recorder.ring_buffer, &audio_recorder.ring_buffer, None).unwrap()

}