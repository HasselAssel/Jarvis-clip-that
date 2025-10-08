use std::thread;
use crossbeam::channel::{unbounded, select};

use rdev::Key;

use crate::recorder::clipper::key_listener::KeyListener;
use crate::recorder::creator::CompleteRecorder;
use crate::ring_buffer::packet_handlers::KeyFrameStartPacketWrapper;
use crate::ring_buffer::ring_buffer::RingBuffer;
use crate::types::Packet;

mod types;
mod error;
mod ring_buffer;
mod recorder;
mod wrappers;
mod egui;

fn main() {
    test_window_capture_main().unwrap();
}

fn test_window_capture_main() -> Result<(), Box<dyn std::error::Error>> {
    recorder::recorders::test_window_capture::main().unwrap();
    Ok(())
}

fn normal_main() -> Result<(), Box<dyn std::error::Error>> {
    //egui::window::_main().expect("TODO: panic message");

    type VPRB = RingBuffer<KeyFrameStartPacketWrapper>;
    type APRB = RingBuffer<Packet>;

    let mut recorder = CompleteRecorder::<VPRB, APRB>::create_recorder();

    let mut key_listener = KeyListener::new();
    let (sender, receiver) = unbounded();
    key_listener.register_shortcut(&[Key::Alt, Key::KeyM], move || sender.send(()).unwrap());

    recorder.start();
    key_listener.start();

    /*let (sender2, receiver2) = unbounded();
    let voice = start_audio_recording(sender2);
    thread::spawn(move || run_vosk_stream(receiver2));*/


    loop {
        select! {
            recv(receiver) -> _ => {
                recorder.saver.standard_save_to_disc(&recorder.video_recorder.1, &recorder.audio_recoders[0].1, None).unwrap()
            }
        }
    }
}