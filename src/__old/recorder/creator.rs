use std::any::Any;
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

use ffmpeg_next::codec::{Flags, Parameters};
use ffmpeg_next::sys::AVPixelFormat;

use crate::error::Error;
use crate::error::Error::Unknown;
use crate::recorder::clipper::saver::Saver;
use crate::recorder::parameters::{AudioParams, BaseParams, VideoParams};
use crate::recorder::recorders::audio::desktop_recorder::AudioCapturer;
use crate::recorder::recorders::d3d11::{create_id3d11device, get_hw_device_and_frame_cxt};
use crate::recorder::recorders::encoders::{AudioEncoderType, AudioFormatType, AudioFormatTypeData, new_audio_encoder, new_video_encoder, VideoEncoderType, VideoFormatType, VideoFormatTypeData};
use crate::recorder::recorders::frame::{create_av_frame, create_frames};
use crate::recorder::recorders::video::desktop_recorder::VideoCapturer;
use crate::recorder::recorders::wasapi::create_default_iaudioclient;
use crate::recorder::traits::Recorder;
use crate::ring_buffer::traits::PacketRingBuffer;
use crate::types::Result;

pub struct CompleteRecorder<VPRB: PacketRingBuffer + 'static, APRB: PacketRingBuffer + 'static> {
    pub video_recorder: (Option<Box<dyn Recorder<VPRB>>>, Arc<Mutex<VPRB>>),
    pub audio_recoders: Vec<(Option<Box<dyn Recorder<APRB>>>, Arc<Mutex<APRB>>)>,

    pub saver: Saver,
}

impl<VPRB: PacketRingBuffer, APRB: PacketRingBuffer> CompleteRecorder<VPRB, APRB> {
    pub fn create_recorder(/*video_frame_type: VideoFrameType, video_encoder_type: VideoEncoderType, video_params: VideoParams*/) -> Self {
        let video_params = VideoParams {
            base_params: BaseParams {
                bit_rate: 8_000_000,
                max_bit_rate: 10_000_000,
                flags: Flags::GLOBAL_HEADER,
                rate: 30, //fps
            },
            out_width: 2560,
            out_height: 1440,
        };

        let audio_params = AudioParams {
            base_params: BaseParams {
                bit_rate: 128_000,
                max_bit_rate: 150_000,
                flags: Flags::GLOBAL_HEADER,
                rate: 48_000,//format.nSamplesPerSec as i32,
            },
            channel_layout: ffmpeg_next::util::channel_layout::ChannelLayout::STEREO,
            format: ffmpeg_next::format::Sample::F32(ffmpeg_next::util::format::sample::Type::Planar),
        };


        let video_format_type = VideoFormatType::D3D11 { monitor_nr: 0 };
        let video_encoder_type = VideoEncoderType::HevcAmf;

        let audio_format_type = AudioFormatType::WasapiSystem;
        let audio_encoder_type = AudioEncoderType::AAC;


        let (video_recorder, video_ring_buffer, video_parameters) = create_video_recorder::<VPRB>(video_format_type, video_encoder_type, video_params).unwrap();
        let (audio_recorder, audio_ring_buffer, audio_parameters) = create_audio_recorder::<APRB>(audio_format_type, audio_encoder_type, audio_params).unwrap().into_iter().next().ok_or(Unknown).unwrap();

        let saver = Saver::new(video_parameters, audio_parameters, "out", "Chat Clip That", ".mp4");

        Self{
            video_recorder: (Some(video_recorder), video_ring_buffer),
            audio_recoders: vec![(Some(audio_recorder), audio_ring_buffer)],

            saver
        }
    }

    pub fn start(&mut self) -> Vec<JoinHandle<Result<()>>> {
        let mut handles = Vec::new();

        if let Some(video_recorder) = self.video_recorder.0.take() {
            handles.push(video_recorder.start_capturing());
        }

        for (ref mut audio_recorder_option, _) in &mut self.audio_recoders {
            if let Some(audio_recorder) = audio_recorder_option.take() {
                handles.push(audio_recorder.start_capturing());
            }
        }

        handles
    }
}

fn create_video_recorder<PRB: PacketRingBuffer + 'static>(video_format_type: VideoFormatType, video_encoder_type: VideoEncoderType, video_params: VideoParams) -> Result<(Box<dyn Recorder<PRB>>, Arc<Mutex<PRB>>, Parameters)> {
    let create_rb = || -> Arc<Mutex<PRB>> {
        let ring_buffer = PRB::new((60 * video_params.base_params.rate) as u32);
        Arc::new(Mutex::new(ring_buffer))
    };

    let (recorder, arc_ring_buffer, parameters) = match video_format_type {
        VideoFormatType::D3D11 { monitor_nr } => {
            let arc_ring_buffer = create_rb();
            let (device, context, duplication) = create_id3d11device(monitor_nr).unwrap();
            let (hw_device_ctx, hw_frame_ctx) = get_hw_device_and_frame_cxt(&device, &video_params);
            let video_frame_type_data = VideoFormatTypeData::D3D11 { hw_device_ctx, hw_frame_ctx };
            let video_encoder = new_video_encoder(&video_params, &video_frame_type_data, &video_encoder_type).unwrap();
            let video_parameters = Parameters::from(&video_encoder);
            let av_frame = create_av_frame(AVPixelFormat::AV_PIX_FMT_D3D11, video_params.out_width as i32, video_params.out_height as i32, hw_frame_ctx).unwrap();
            let desktop_recorder = VideoCapturer::new(arc_ring_buffer.clone(), video_encoder, video_params, (device, context, duplication), av_frame);
            (desktop_recorder, arc_ring_buffer, video_parameters)
        }
        _ => return Err(Error::NonExistentParameterCombination.into())
    };

    Ok((Box::new(recorder), arc_ring_buffer, parameters))
}

fn create_audio_recorder<PRB: PacketRingBuffer + 'static>(audio_format_type: AudioFormatType, audio_encoder_type: AudioEncoderType, audio_params: AudioParams) -> Result<Vec<(Box<dyn Recorder<PRB>>, Arc<Mutex<PRB>>, Parameters)>> {
    let create_rb = || -> Arc<Mutex<PRB>> {
        let ring_buffer = PRB::new((60 * audio_params.base_params.rate) as u32);
        Arc::new(Mutex::new(ring_buffer))
    };

    let vec: Vec<(Box<dyn Recorder<PRB>>, Arc<Mutex<PRB>>, Parameters)> =
        match audio_format_type {
            AudioFormatType::WasapiSystem => {
                let arc_ring_buffer = create_rb();
                let (client, format) = create_default_iaudioclient().unwrap();
                let audio_frame_type_data = AudioFormatTypeData::WasapiSystem;
                let audio_encoder = new_audio_encoder(&audio_params, &audio_frame_type_data, &audio_encoder_type).unwrap();
                let audio_parameters = Parameters::from(&audio_encoder);
                let (frame, silent_frame) = create_frames(audio_encoder.format(), audio_encoder_type as usize, audio_encoder.channel_layout());
                let desktop_recorder = AudioCapturer::new(arc_ring_buffer.clone(), audio_encoder, audio_params, client, format, frame, silent_frame);
                vec![(Box::new(desktop_recorder), arc_ring_buffer.clone(), audio_parameters)]
            }
            AudioFormatType::WasapiClient => {
                return Err(Error::NotYetImplemented.into());
            }
            _ => return Err(Error::NonExistentParameterCombination.into())
        };

    Ok(vec)
}