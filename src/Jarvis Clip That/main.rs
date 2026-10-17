use anyhow::Context;

use crate::config::AppConfig;
use crate::key_listener::KeyListener;
use crate::recorders::factory::{Factory, FactoryCommands, FactoryResponse};
use crate::recorders::traits::{Converter, Encoder, Source};
use crate::save::{Saver, SaveStream};

mod recorders;
#[path = "../macros.rs"]
mod macros;
mod save;
mod key_listener;
mod config;

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
        if let FactoryResponse::Save(streams) = resp_recv.recv().unwrap() {
            dbg!(streams.len());
            let save_streams = streams.into_iter().map(SaveStream::from).collect::<Vec<_>>();
            dbg!(save_streams.len());

            save_streams.iter().for_each(|ss| { dbg!(ss.packets.len()); });
            save_streams.iter().for_each(|ss| { dbg!(ss.time_base); });

            let path = saver.save(save_streams).unwrap();
            dbg!(path);
        }
    });

    key_listener.start();


    /*let retention_secs = 30;
    let (cmd_send, cmd_recv) = crossbeam_channel::unbounded();
    let (resp_send, resp_recv) = crossbeam_channel::unbounded();
    
    let factory = Factory::new(retention_secs, cmd_recv, resp_send);
    factory.start();
    
    let video_config = Config {
        frame_config: Video(VideoConfig {
            fps: 30,
            width: 2560,
            height: 1440,
            environment: D3D11 {monitor: 0},
            codec: VideoCodec::HevcAmf,
        }),
    };

    let audio_config = Config {
        frame_config: Audio(AudioConfig {
            environment: WasAPI(WasApiDefaultSys),
            codec: AudioCodec::AAC,
        }),
    };

    cmd_send.send(FactoryCommands::AddRecorder(video_config)).unwrap();
    cmd_send.send(FactoryCommands::AddRecorder(audio_config)).unwrap();

    let mut saver = Saver::new("out").unwrap();

    let mut key_listener = KeyListener::new();
    key_listener.register_shortcut(&[Key::Alt, Key::KeyM], move || {
        cmd_send.send(FactoryCommands::Save).unwrap();
        if let FactoryResponse::Save(streams) = resp_recv.recv().unwrap() {
            dbg!(streams.len());
            let save_streams = streams.iter().map(SaveStream::from).collect::<Vec<_>>();
            dbg!(save_streams.len());

            save_streams.iter().for_each(|ss| { dbg!(ss.packets.len()); });
            save_streams.iter().for_each(|ss| { dbg!(ss.time_base); });

            let path = saver.save(&save_streams).unwrap();
            dbg!(path);
        }
    });

    key_listener.start();*/
}