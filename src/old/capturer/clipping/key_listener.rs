/*use std::thread;
use std::thread::JoinHandle;

#[derive(Default)]
struct KeyFlags {
    ctrl: bool,
    n: bool,
}

pub struct KeyListener {
    key_flags: KeyFlags,
    action: fn(),
}

impl KeyListener {
    pub fn new(action: fn()) -> Self {
        Self {
            key_flags: KeyFlags::default(),
            action,
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
                    (self.action)()//.standard_save(None).unwrap();
                }
            };

            if let Err(err) = rdev::listen(handler) {
                eprintln!("Error: {:?}", err);
            }
        })
    }
}*/

use rdev::{listen, Event, EventType, Key};
use std::collections::HashSet;
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};

type Callback = Box<dyn Fn() + Send + 'static>;

struct Shortcut {
    keys: HashSet<Key>,
    action: Callback,
}

pub struct KeyListener {
    shortcuts: Arc<Mutex<Vec<Shortcut>>>,
    pressed_keys: Arc<Mutex<HashSet<Key>>>,
}

impl KeyListener {
    pub fn new() -> Self {
        Self {
            shortcuts: Arc::new(Mutex::new(Vec::new())),
            pressed_keys: Arc::new(Mutex::new(HashSet::new())),
        }
    }

    pub fn register_shortcut<F>(&mut self, keys: &[Key], action: F)
    where
        F: Fn() + Send + 'static,
    {
        let shortcut = Shortcut {
            keys: keys.iter().cloned().collect(),
            action: Box::new(action),
        };
        self.shortcuts.lock().unwrap().push(shortcut);
    }

    pub fn reset_keys(&self) {
        self.pressed_keys.lock().unwrap().clear();
    }

    pub fn start(&self) -> JoinHandle<()> {
        let shortcuts = self.shortcuts.clone();
        let pressed_keys = self.pressed_keys.clone();

        thread::spawn(move || {
            let handler = move |event: Event| {
                let key = match event.event_type {
                    EventType::KeyPress(k) => {
                        pressed_keys.lock().unwrap().insert(k);
                        Some(k)
                    }
                    EventType::KeyRelease(k) => {
                        pressed_keys.lock().unwrap().remove(&k);
                        None
                    }
                    _ => None,
                };

                if key.is_some() {
                    let current_keys = pressed_keys.lock().unwrap().clone();
                    for shortcut in shortcuts.lock().unwrap().iter() {
                        if shortcut.keys.is_subset(&current_keys) {
                            (shortcut.action)();
                        }
                    }
                }
            };

            if let Err(e) = listen(handler) {
                eprintln!("Error: {:?}", e);
            }
        })
    }
}
