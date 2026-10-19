use anyhow::Context;

use crate::config::AppConfig;
use crate::key_listener::KeyListener;
use crate::recorders::factory::{Factory, FactoryCommands, FactoryResponse};
use crate::recorders::traits::{Converter, Encoder, Source};
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

    key_listener.register_shortcut(keys, move || {
        cmd_send.send(FactoryCommands::Save).unwrap();
        if let FactoryResponse::Save(save, ) = resp_recv.recv().unwrap() {
            let path = saver.save(save).unwrap();
            debug!(path);
        }
    });

    key_listener.start();
}
