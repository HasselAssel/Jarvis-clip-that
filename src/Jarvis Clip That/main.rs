use std::thread;

use anyhow::Context;

use crate::config::AppConfig;
use crate::key_listener::KeyListener;
use crate::recorders::factory::{Factory, FactoryCommands, FactoryResponse};
use crate::recorders::traits::{Converter, Encoder, Source};
use crate::recorders::windows::wasapi::process_watcher::WasApiProcessWatcher;
use crate::save::Saver;

mod recorders;
#[path = "../macros.rs"]
mod macros;
mod save;
mod key_listener;
mod config;
mod timing;

fn main() {
    let config_path = "config/config.toml";
    let app_config = AppConfig::load(config_path).expect(&format!("Invalid config, check {:?}", config_path));

    let (cmd_send, cmd_recv) = crossbeam_channel::unbounded();
    let (resp_send, resp_recv) = crossbeam_channel::unbounded();

    let factory = Factory::new(app_config.retention_secs, cmd_recv, resp_send);
    factory.start();

    for recorder_config in app_config.recorders {
        cmd_send.send(FactoryCommands::AddRecorder(recorder_config)).unwrap();
    }

    let mut key_listener = KeyListener::new();

    let mut saver = Saver::new(app_config.output_dir).unwrap();

    let keys = app_config.keybinds.save_clip.iter().map(|keys| keys.iter().cloned().collect()).collect();
    let cmd_send_k = cmd_send.clone();
    key_listener.register_shortcut(keys, move || {
        cmd_send_k.send(FactoryCommands::Save).unwrap();
        if let FactoryResponse::Save(save) = resp_recv.recv().unwrap() {
            let path = saver.save(save).unwrap();
            debug!(path);
        }
    });

    thread::spawn(move || {
        key_listener.start();
    });

    if let Some((codec, include_tree)) = app_config.watch_processes {
        let watcher = WasApiProcessWatcher::new(codec, include_tree, cmd_send).expect("Can't create watcher");
        watcher.start().expect("Watcher Panicked");
    }

    thread::park();
}
