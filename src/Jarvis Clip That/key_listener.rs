use std::collections::HashSet;

use rdev::Event;
use rdev::EventType;
use rdev::Key;
use rdev::listen;

struct Shortcuts {
    keys_sets: Vec<HashSet<Key>>,
    action: Box<dyn FnMut() + Send + 'static>,
}

pub struct KeyListener {
    shortcuts: Vec<Shortcuts>,
    pressed_keys: HashSet<Key>,
}

impl KeyListener {
    pub fn new() -> Self {
        Self {
            shortcuts: Vec::new(),
            pressed_keys: HashSet::new(),
        }
    }

    pub fn register_shortcut<F>(&mut self, keys_sets: Vec<HashSet<Key>>, action: F)
    where
        F: FnMut() + Send + 'static,
    {
        let shortcut = Shortcuts {
            keys_sets,
            action: Box::new(action),
        };
        self.shortcuts.push(shortcut);
    }

    pub fn start(mut self) {
        let handler = move |event: Event| {
            let key = match event.event_type {
                EventType::KeyPress(k) => {
                    self.pressed_keys.insert(k);
                    Some(k)
                }
                EventType::KeyRelease(k) => {
                    self.pressed_keys.remove(&k);
                    None
                }
                _ => None,
            };

            if key.is_some() {
                'outer: for shortcut in self.shortcuts.iter_mut() {
                    for key_combination in &shortcut.keys_sets {
                        if key_combination.is_subset(&self.pressed_keys) {
                            (shortcut.action)();
                            continue 'outer
                        }
                    }
                }
            }
        };

        if let Err(e) = listen(handler) {
            eprintln!("Key listener Error: {:?}", e);
        }
    }
}
