use std::thread;
use std::thread::JoinHandle;
use crate::capturer::saver::Saver;


#[derive(Default)]
struct KeyFlags {
    ctrl: bool,
    n: bool,
}

pub struct KeyListener {
    key_flags: KeyFlags,
    saver: Saver,
}

impl KeyListener {
    pub fn new(saver: Saver) -> Self {
        Self {
            saver,
            key_flags: KeyFlags::default(),
        }
    }

    pub fn start_key_listener(mut self) -> JoinHandle<()> {
        thread::spawn(move || {
            let handler = move |event: rdev::Event| {
                match event.event_type {
                    rdev::EventType::KeyPress(rdev::Key::ControlLeft) => self.key_flags.ctrl = true,
                    rdev::EventType::KeyRelease(rdev::Key::ControlLeft) => self.key_flags.ctrl = false,
                    rdev::EventType::KeyPress(rdev::Key::KeyN) => self.key_flags.n = true,
                    rdev::EventType::KeyRelease(rdev::Key::KeyN) => self.key_flags.n = false,
                    _ => {}
                }

                if self.key_flags.ctrl && self.key_flags.n {
                    self.key_flags.n = false;
                    self.saver.standard_save(None).unwrap();
                }
            };

            if let Err(err) = rdev::listen(handler) {
                eprintln!("Error: {:?}", err);
            }
        })
    }
}