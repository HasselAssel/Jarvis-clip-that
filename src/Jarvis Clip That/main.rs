use rdev::Key;
use crate::recorders::audio::sources::enums::{AudioCodec, AudioSourceType};
use crate::recorders::audio::sources::wasapi::source::AudioProcessWatcher;
use crate::recorders::recorder::{create_audio_recorder, create_video_recorder};
use crate::recorders::save::key_listener::KeyListener;
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
#[path = "../shared_macros.rs"]
mod shared_macros;

#[tokio::main]
async fn main() {
    main_sync().await
}

type VideoPacketRingBufferType = RingBuffer<KeyFrameStartPacketWrapper>;
type AudioPacketRingBufferType = RingBuffer<Packet>;

async fn main_sync() {
    let video_source_type = VideoSourceType::D3d11 { monitor_id: 0 };
    let video_codec = VideoCodec::Amf;

    let audio_source_type = AudioSourceType::WasApiDefaultInput;
    let audio_codec = AudioCodec::AAC;


    let seconds = 30;
    let fps = 30;

    let mut video_recorder = create_video_recorder::<VideoPacketRingBufferType>(&video_source_type, &video_codec, seconds, 2560, 1440, fps).unwrap();
    let mut audio_recorder_input = create_audio_recorder::<AudioPacketRingBufferType>(&audio_source_type, &audio_codec, seconds).unwrap();
    let mut audio_recorder = AudioProcessWatcher::<AudioPacketRingBufferType>::new(audio_codec, true, seconds);


    let save_env = SaverEnv::new("out", "Chat Clip That", Some("sounds/BOOM.mp3"));


    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<()>();


    let mut key_listener = KeyListener::new();
    key_listener.register_shortcut(&[Key::Alt, Key::KeyM], move || {
        debug_println!("OK GARMIN VIDEO SPEICHERN");
        tx.send(()).unwrap();
    });

    key_listener.start();


    video_recorder.start_recording(None);
    audio_recorder.start_recording(None).await;
    audio_recorder_input.start_recording(None);

    loop {
        tokio::select! {
            Some(_) = rx.recv() => {
                let mut save = save_env.new_save::<String>(None);

                save.add_stream(&video_recorder.ring_buffer, &video_recorder.parameters, true).unwrap();
                save.add_stream(&audio_recorder_input.ring_buffer, &audio_recorder_input.parameters, false).unwrap();

                for (p_id, (recorder, _, _)) in audio_recorder.audio_recorders.lock().await.iter() {
                    debug_println!("stream added for: {}", p_id);
                    save.add_stream(&recorder.ring_buffer, &recorder.parameters, false).unwrap();
                }

                save.finalize_and_save().unwrap();
            },
            else => break,
        }
    }
}