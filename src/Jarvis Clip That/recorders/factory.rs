use std::thread;
use std::time::Duration;

use anyhow::{anyhow, Result};
use crossbeam_channel::{Receiver, Select, Sender};
use ffmpeg_next::{codec, Packet, Rational};
use ffmpeg_next::encoder::find_by_name;
use ffmpeg_next::ffi::av_rescale_q;
use ffmpeg_next::format::Sample;
use ffmpeg_next::format::sample::Type::Planar;

use crate::debug;
use crate::recorders::config::{AudioCodec, Config, FrameConfig, VideoCodec};
use crate::recorders::driver::{EventDriver, PollingDriver};
use crate::recorders::recorder::{RecorderPipeline, RecorderStream};
use crate::recorders::ringbuffer::{KeyFrameStartPacketWrapper, RingBuffer};
use crate::recorders::windows::{AudioEnvironment, VideoEnvironment, WasAPI};
use crate::recorders::windows::d3d11::converters::BgraToNv12Converter;
use crate::recorders::windows::d3d11::encoders::EncoderD3D11;
use crate::recorders::windows::d3d11::env::EnvD3D11;
use crate::recorders::windows::d3d11::sources::SourceD3d11;
use crate::recorders::windows::wasapi::converter::ConverterWasapi;
use crate::recorders::windows::wasapi::encoder::EncoderWasapi;
use crate::recorders::windows::wasapi::env::EnvWasapi;
use crate::recorders::windows::wasapi::sources::SourceWasapi;
use crate::save::SaveStream;
use crate::timing::{Moment, now};

fn create_start_recorder(config: Config, retention_secs: i64, creation_moment: Moment) -> Result<RecorderStream<Vec<Packet>>> {
    let (sender, receiver) = crossbeam_channel::unbounded();
    match config.frame_config {
        FrameConfig::Video(video_config) => {
            match video_config.environment {
                VideoEnvironment::D3D11 { monitor } => {
                    let env = EnvD3D11::new()?;
                    let source = SourceD3d11::new(
                        monitor,
                        &env
                    )?;
                    let converter = BgraToNv12Converter::new(
                        (2560, 1440), // TODO!!!
                        (2560, 1440), // TODO!!!
                        &env
                    )?;
                    let codec = match video_config.codec {
                        VideoCodec::HevcAmf => find_by_name("hevc_amf").ok_or_else(|| anyhow!("no codec!"))?
                    };
                    let ctx = codec::context::Context::new_with_codec(codec);
                    let enc = ctx.encoder().video()?;
                    let encoder = EncoderD3D11::new(
                        (2560, 1440), // TODO!!!
                        video_config.fps,
                        enc,
                        codec,
                        creation_moment.clone(),
                        &env
                    )?;
                    let driver = PollingDriver::new(Duration::from_secs_f64(1f64 / video_config.fps as f64));
                    let params = encoder.get_params();
                    let time_base = Rational(1, video_config.fps);
                    let recorder = RecorderPipeline::<
                        EnvD3D11,_,SourceD3d11,BgraToNv12Converter,EncoderD3D11,
                    >::new(env, driver, source, converter, encoder);
                    let now_rescaled = unsafe {
                        av_rescale_q(
                            creation_moment.pos,
                            Rational::from((creation_moment.time_base.0 as i32, creation_moment.time_base.1 as i32)).into(),
                            time_base.into(),
                        )
                    };
                    let rs = RecorderStream::new::<RingBuffer<KeyFrameStartPacketWrapper>>(
                        receiver,
                        retention_secs,
                        params,
                        time_base,
                        now_rescaled,
                        config.name
                    );
                    recorder.start(sender);

                    Ok(rs)
                }
            }
        }
        FrameConfig::Audio(audio_config) => {
            match audio_config.environment {
                AudioEnvironment::WasApi(wasapi) => {
                    let (p_id, rec, ipt) = match wasapi {
                        WasAPI::WasApiDefaultSys => { (None, true, false) }
                        WasAPI::WasApiProcess { process_id, include_tree } => { (Some(process_id), true, include_tree) }
                        WasAPI::WasApiDefaultInput => { (None, false, false) }
                    };
                    let env = EnvWasapi::new(p_id, rec, ipt)?;
                    let source = SourceWasapi::new(
                        env.client.clone(),
                    )?;
                    let converter = ConverterWasapi::new(
                        None,
                        &env,
                    );
                    let (codec, frame_size, sample) = match audio_config.codec {
                        AudioCodec::Aac => (find_by_name("aac").ok_or_else(|| anyhow!("no codec!"))?, 1024, Sample::F32(Planar))
                    };
                    let ctx = codec::context::Context::new_with_codec(codec);
                    let enc = ctx.encoder().audio()?;
                    let time_base = Rational(1, env.format.format().nSamplesPerSec as i32);
                    let now_rescaled_buffer_qpc = unsafe {
                        av_rescale_q(
                            creation_moment.pos,
                            Rational::from((creation_moment.time_base.0 as i32, creation_moment.time_base.1 as i32)).into(),
                            Rational(1, 10_000_000).into(), // GetBuffer always returns qpc in (1, 10_000_000) time_base :)
                        )
                    };
                    let encoder = EncoderWasapi::new(
                        enc,
                        codec,
                        frame_size,
                        sample,
                        now_rescaled_buffer_qpc as i128,
                        &env,
                    )?;
                    let driver = EventDriver;
                    let params = encoder.get_params();
                    let recorder = RecorderPipeline::<
                        EnvWasapi,_,SourceWasapi,ConverterWasapi,EncoderWasapi,
                    >::new(env, driver, source, converter, encoder);
                    let now_rescaled = unsafe {
                        av_rescale_q(
                            creation_moment.pos,
                            Rational::from((creation_moment.time_base.0 as i32, creation_moment.time_base.1 as i32)).into(),
                            time_base.into(),
                        )
                    };
                    let rs = RecorderStream::new::<RingBuffer<Packet>>(
                        receiver,
                        retention_secs,
                        params,
                        time_base,
                        now_rescaled,
                        config.name,
                    );
                    recorder.start(sender);
                    Ok(rs)
                }
            }
        }
    }
}

pub enum FactoryCommands {
    Save,
    AddRecorder(Config),
}

pub enum FactoryResponse {
    Save(CommandSave),
}

pub struct Factory {
    recorder_streams: Vec<RecorderStream<Vec<Packet>>>,
    retention_secs: i64,

    command_receiver: Receiver<FactoryCommands>,
    response_sender: Sender<FactoryResponse>,
}

impl Factory {
    pub fn new(
        retention_secs: i64,
        command_receiver: Receiver<FactoryCommands>,
        response_sender: Sender<FactoryResponse>
    ) -> Self {
        Self {
            recorder_streams: Vec::new(),
            retention_secs,
            command_receiver,
            response_sender,
        }
    }

    pub fn start(mut self) {
        thread::spawn(move || {
            loop {
                let event = {
                    let mut select = Select::new();

                    select.recv(&self.command_receiver);

                    for stream in self.recorder_streams.iter() {
                        select.recv(&stream.receiver);
                    }

                    let operation = select.select();
                    let mut index = operation.index();

                    if index == 0 { // command received // todo cleanup
                        let result = operation.recv(&self.command_receiver);
                        if let Ok(command) = result {
                            self.handle_command(command);
                        }
                        // TODO: inf loop Err from command receiver if all sender dropped
                        continue
                    }

                    index -= 1;

                    let result = operation.recv(&self.recorder_streams[index].receiver);

                    (index, result)
                };

                let (index, result) = event;

                match result {
                    Ok(packets) => {
                        self.recorder_streams[index].insert(packets);
                    }
                    Err(_) => {
                        self.recorder_streams.swap_remove(index);
                    }
                }
            }
        });
    }

    fn handle_command(&mut self, command: FactoryCommands) {
        match command {
            FactoryCommands::Save => {
                let moment = now().expect("no time :( for saving");
                let mut save_streams = Vec::with_capacity(self.recorder_streams.len());
                self.recorder_streams.iter().for_each(|stream|
                    save_streams.push(SaveStream {
                        packets: stream.copy_out(),
                        parameters: stream.parameters.clone(),
                        time_base: stream.time_base,
                        start_time: stream.start_time,
                        name: stream.name.clone(),
                    })
                );
                let cmd_save = CommandSave {
                    streams: save_streams,
                    time_stamp: moment.pos,
                    time_base: moment.time_base,
                    retention_secs: self.retention_secs,
                };
                let _ = self.response_sender.try_send(FactoryResponse::Save(cmd_save)); // todo: proper handling
            }

            FactoryCommands::AddRecorder(config) => {
                debug!(&config);
                let moment = now().expect("no time :( for adding");
                let res = create_start_recorder(config, self.retention_secs, moment);
                if let Ok(recorder_stream) = res {
                    self.recorder_streams.push(recorder_stream);
                } else {
                    debug!(res.err());
                }
            }
        }
    }
}

pub struct CommandSave {
    pub streams: Vec<SaveStream>,
    pub time_stamp: i64,
    pub time_base: (i64, i64),
    pub retention_secs: i64,
}