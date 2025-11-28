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
    main_async().await
}

type VideoPacketRingBufferType = RingBuffer<KeyFrameStartPacketWrapper>;
type AudioPacketRingBufferType = RingBuffer<Packet>;

async fn main_async() {
    let video_source_type = VideoSourceType::D3d11 { monitor_id: 0 };
    let video_codec = VideoCodec::Amf;

    let audio_source_type = AudioSourceType::WasApiDefaultInput;
    let audio_codec = AudioCodec::AAC;


    let seconds = 5;
    let fps = 30;

    let mut video_recorder = create_video_recorder::<VideoPacketRingBufferType>(&video_source_type, &video_codec, seconds, 2560, 1440, fps, 0.).unwrap();
    let mut audio_recorder_input = create_audio_recorder::<AudioPacketRingBufferType>(&audio_source_type, &audio_codec, seconds, 0.).unwrap();
    let mut audio_recorder = AudioProcessWatcher::<AudioPacketRingBufferType>::new(audio_codec, true, seconds, 0.).unwrap();


    let save_env = SaverEnv::new("out", "Chat Clip That", Some("sounds/BOOM.mp3"));


    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<()>();


    let mut key_listener = KeyListener::new();
    key_listener.register_shortcut(&[Key::Alt, Key::KeyM], move || {
        eprintln!("OK GARMIN VIDEO SPEICHERN");
        if let Err(_) = tx.send(()) {
            eprintln!("Key responder died :(")
        }
    });

    key_listener.start();


    video_recorder.start_recording(None);
    audio_recorder_input.start_recording(None);
    audio_recorder.start_recording().await.unwrap_or_else(|err| panic!("Failed start_recording because: {:?}", err));

    loop {
        tokio::select! {
            Some(_) = rx.recv() => {
                if let Ok(mut save) = save_env.new_save::<String>(None){
                    save.add_stream(&video_recorder.ring_buffer, &video_recorder.parameters, true, Some("Main Video")).unwrap();
                    save.add_stream(&audio_recorder_input.ring_buffer, &audio_recorder_input.parameters, false, Some("Main Audio")).unwrap();

                    for (p_id, (recorder, titel, _)) in audio_recorder.audio_recorders.lock().await.iter() {
                        debug_println!("stream added for: {}", p_id);
                        save.add_stream(&recorder.ring_buffer, &recorder.parameters, false, Some(titel)).unwrap();
                    }

                    if let Err(error) = save.finalize_and_save() {
                        eprintln!("Couldn't save clip: {:?}", error);
                    }
                }

            },
            else => break,
        }
    }
}