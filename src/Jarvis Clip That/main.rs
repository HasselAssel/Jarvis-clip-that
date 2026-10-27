use std::{fs, thread};
use std::io::Cursor;
use std::sync::Arc;

use rodio::{Decoder, DeviceSinkBuilder, Player};

use crate::config::AppConfig;
use crate::key_listener::KeyListener;
use crate::recorders::factory::{Factory, FactoryCommands, FactoryResponse};
#[cfg(target_os = "windows")]
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
    ffmpeg_next::init().unwrap_or_else(|e| panic!("ffmpeg_next init failed: {e}"));

    println!("Welcome to Jarvis Clip That");

    let config_path = "config/config.toml";
    let app_config = AppConfig::load(config_path).expect(&format!("Invalid config, check {:?}", config_path));

    println!(
        "Press {} to clip the last {} seconds!",
        app_config.keybinds.save_clip.iter()
            .map(|keys| format!("({})",
                                keys.iter()
                                    .map(|key| format!("{key:?}"))
                                    .collect::<Vec<_>>()
                                    .join(" + ")
            ))
            .collect::<Vec<_>>()
            .join(" or "),
        app_config.retention_secs
    );

    println!("View your saved clips in \"{}\"", app_config.output_dir);

    let (cmd_send, cmd_recv) = crossbeam_channel::unbounded();
    let (resp_send, resp_recv) = crossbeam_channel::unbounded();

    let factory = Factory::new(app_config.retention_secs, cmd_recv, resp_send);
    thread::spawn(move || {
        factory.start();
    });

    for recorder_config in app_config.recorders {
        cmd_send.send(FactoryCommands::AddRecorder(recorder_config)).unwrap();
    }

    let mut key_listener = KeyListener::new();

    let mut saver = Saver::new(app_config.output_dir).unwrap();

    let keys = app_config.keybinds.save_clip
        .iter()
        .map(|keys| keys.iter().cloned().collect())
        .collect();

    let save_sound = app_config.keybinds.save_clip_sound_file
        .and_then(|file| fs::read(file).ok())
        .and_then(|buffer|
            Some((
                Arc::from(buffer),
                app_config.keybinds.save_clip_sound_volume.unwrap_or(1.)
            ))
    );

    let cmd_send_k = cmd_send.clone();
    key_listener.register_shortcut(keys, move || {
        cmd_send_k.send(FactoryCommands::Save).unwrap();
        let FactoryResponse::Save(save) = resp_recv.recv().unwrap(); {
            match saver.save(save) {
                Ok(path) => {
                    if let Some((save_sound_buffer, save_sound_volume)) = save_sound.clone() {
                        thread::spawn(move || {
                            let source = Decoder::new(Cursor::new(save_sound_buffer))?;
                            let mut stream = DeviceSinkBuilder::open_default_sink()?;
                            stream.log_on_drop(false);
                            let player = Player::connect_new(stream.mixer());
                            player.append(source);
                            player.set_volume(save_sound_volume);
                            player.sleep_until_end();
                            Ok::<(), anyhow::Error>(())
                        });
                    }

                    println!("Saved clip to {:?}", path);
                }
                Err(err) => {
                    debug!(err);
                }
            }
        }
    });

    thread::spawn(move || {
        key_listener.start();
    });

    #[cfg(target_os = "windows")]
    if let Some((codec, include_tree)) = app_config.watch_processes {
        let watcher = WasApiProcessWatcher::new(codec, include_tree, cmd_send).expect("Can't create watcher");
        watcher.start().expect("Watcher Panicked");
    }

    thread::park();
}
