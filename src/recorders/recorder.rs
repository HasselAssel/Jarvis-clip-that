use std::sync::{Arc, Mutex};
use std::sync::atomic::AtomicBool;
use ffmpeg_next::ChannelLayout;
use ffmpeg_next::codec::Parameters;

use ffmpeg_next::encoder::find_by_name;
use ffmpeg_next::format::Sample;
use ffmpeg_next::format::sample::Type;
use ffmpeg_next::sys::AVPixelFormat::{AV_PIX_FMT_D3D11, AV_PIX_FMT_QSV};
use crate::error::Error::Unknown;
use crate::recorders::audio::audio_recorder::AudioRecorder;
use crate::recorders::audio::sources::enums::{AudioCodec, AudioSourceType};
use crate::recorders::audio::sources::wasapi::aac::{AAC_FRAME_SIZE, AacContext};
use crate::recorders::audio::sources::wasapi::source::{AudioProcessWatcher, AudioSourceWasapi};
use crate::recorders::audio::sources::wasapi::traits::new_audio_encoder_aac;

use crate::recorders::frame::{create_audio_frames, create_av_frame};
use crate::recorders::traits::TRecorder;
use crate::recorders::video::video_recorder::VideoRecorder;
use crate::recorders::video::sources::d3d111::d3d11av::D3d11vaAdapter;
use crate::recorders::video::sources::d3d111::qsv::QsvAdapter;
use crate::recorders::video::sources::d3d111::source::VideoSourceD3d11;
use crate::recorders::video::sources::d3d111::traits::{create_encoder_d3d11, D3d11EncoderHwContext};
use crate::recorders::video::sources::enums::{VideoCodec, VideoSourceType};
use crate::ring_buffer::traits::PacketRingBuffer;
use crate::types::{RecorderJoinHandle, Result};
use crate::wrappers::MaybeSafeFFIPtrWrapper;

pub struct Recorder<PRB: PacketRingBuffer> {
    recorder: Option<Box<dyn TRecorder<PRB> + Send>>,
    pub ring_buffer: Arc<Mutex<PRB>>,
    pub parameters: Parameters,
}

impl<PRB: PacketRingBuffer> Recorder<PRB> {
    fn from_recorder<R: TRecorder<PRB> + 'static + Send>(recorder: R, ring_buffer: Arc<Mutex<PRB>>, parameters: Parameters) -> Self {
        let recorder = Box::new(recorder);
        Self::from_boxed_recorder(recorder, ring_buffer, parameters)
    }

    fn from_boxed_recorder(recorder: Box<dyn TRecorder<PRB> + Send>, ring_buffer: Arc<Mutex<PRB>>, parameters: Parameters) -> Self {
        let recorder = Some(recorder);
        Self {
            recorder,
            ring_buffer,
            parameters,
        }
    }

    pub fn start_recording(&mut self, stop_capturing_callback: Option<Arc<AtomicBool>>) -> Option<RecorderJoinHandle> {
        self.recorder.take().map(|recorder| {
            recorder.start_capturing(stop_capturing_callback)
        })
    }
}


pub fn create_video_recorder<PRB: PacketRingBuffer + 'static>(video_source_type: &VideoSourceType, video_codec: &VideoCodec, min_secs: u32, width: u32, height: u32, fps: i32) -> Result<Recorder<PRB>> {
    let ring_buffer = PRB::new(min_secs * fps as u32);
    let arc_ring_buffer = Arc::new(Mutex::new(ring_buffer));

    let codec = match video_codec {
        VideoCodec::Amf => { find_by_name("hevc_amf").ok_or(ffmpeg_next::Error::EncoderNotFound)? }
        VideoCodec::Qsv => { return Err(ffmpeg_next::Error::EncoderNotFound.into()); }
    };
    let ctx = ffmpeg_next::codec::context::Context::new_with_codec(codec);
    let mut enc = ctx.encoder().video().unwrap();

    let idk = match video_source_type {
        VideoSourceType::D3d11 { monitor_id } => {
            match video_codec {
                VideoCodec::Amf => {
                    let d3d11_vs = VideoSourceD3d11::new(*monitor_id, D3d11vaAdapter);
                    let (hw_device_ctx, hw_frame_ctx) = d3d11_vs.encoder_hw_ctx.setup_hw_and_frame_ctx(&d3d11_vs.device, width as i32, height as i32).unwrap();
                    let encoder = create_encoder_d3d11(enc, codec, (hw_device_ctx, hw_frame_ctx), width, height, fps)?;
                    let parameters = Parameters::from(&encoder);
                    let av_frame = create_av_frame(AV_PIX_FMT_D3D11, width as i32, height as i32, hw_frame_ctx)?;
                    let recorder = VideoRecorder::new(arc_ring_buffer.clone(), d3d11_vs, encoder, MaybeSafeFFIPtrWrapper(av_frame), width, height, fps as f64);
                    Recorder::from_recorder(recorder, arc_ring_buffer, parameters)
                }
                VideoCodec::Qsv => {
                    let d3d11_vs = VideoSourceD3d11::new(*monitor_id, QsvAdapter);
                    let (hw_device_ctx, hw_frame_ctx) = d3d11_vs.encoder_hw_ctx.setup_hw_and_frame_ctx(&d3d11_vs.device, width as i32, height as i32).unwrap();
                    let encoder = create_encoder_d3d11(enc, codec, (hw_device_ctx, hw_frame_ctx), width, height, fps)?;
                    let parameters = Parameters::from(&encoder);
                    let av_frame = create_av_frame(AV_PIX_FMT_QSV, width as i32, height as i32, hw_frame_ctx)?;
                    let recorder = VideoRecorder::new(arc_ring_buffer.clone(), d3d11_vs, encoder, MaybeSafeFFIPtrWrapper(av_frame), width, height, fps as f64);
                    Recorder::from_recorder(recorder, arc_ring_buffer, parameters)
                }
            }
        }
        VideoSourceType::TEST => {
            todo!()
        }
    };
    Ok(idk)
}


pub fn create_audio_recorder<PRB: PacketRingBuffer + 'static>(audio_source_type: &AudioSourceType, audio_code_c: &AudioCodec, min_secs: u32) -> Result<Recorder<PRB>> {
    let create_ring_buffer = |rate| -> Arc<Mutex<PRB>>{
        let ring_buffer = PRB::new(min_secs * rate);
        Arc::new(Mutex::new(ring_buffer))
    };

    let codec = match audio_code_c {
        AudioCodec::AAC => { ffmpeg_next::codec::encoder::find(ffmpeg_next::codec::Id::AAC).ok_or(ffmpeg_next::Error::EncoderNotFound)? }
        AudioCodec::Test => { return Err(ffmpeg_next::Error::EncoderNotFound.into()); }
    };
    let ctx = ffmpeg_next::codec::context::Context::new_with_codec(codec);
    let mut enc = ctx.encoder().audio().unwrap();

    let idk = match audio_source_type {
        AudioSourceType::WasApiDefaultSys | AudioSourceType::WasApiDefaultInput => {
            let render_else_capture = match audio_source_type {
                AudioSourceType::WasApiDefaultSys => { true }
                AudioSourceType::WasApiDefaultInput => { false }
                _ => { unreachable!() }
            };

            match audio_code_c {
                AudioCodec::AAC => {
                    let sample = Sample::F32(Type::Planar);
                    let aac_vs = AudioSourceWasapi::new_default(AacContext, render_else_capture)?;
                    let channel_layout = match aac_vs.format.nChannels {
                        1 => ChannelLayout::MONO,
                        2 => ChannelLayout::STEREO,
                        _ => return Err(Unknown.into())
                    };
                    let (frame, silent_frame) = create_audio_frames(sample, AAC_FRAME_SIZE, channel_layout);
                    let encoder = new_audio_encoder_aac(enc, codec, aac_vs.format.nSamplesPerSec as i32, channel_layout, sample)?;
                    let parameters = Parameters::from(&encoder);
                    let arc_ring_buffer = create_ring_buffer(aac_vs.format.nSamplesPerSec);
                    let recorder = AudioRecorder::new(arc_ring_buffer.clone(), aac_vs, encoder, frame, silent_frame);
                    Recorder::from_recorder(recorder, arc_ring_buffer, parameters)
                }
                AudioCodec::Test => {
                    todo!()
                }
            }
        }
        AudioSourceType::WasApiProcess { process_id, include_tree } => {
            match audio_code_c {
                AudioCodec::AAC => {
                    let sample = Sample::F32(Type::Planar);
                    let aac_vs = AudioSourceWasapi::new_process(AacContext, *process_id, *include_tree)?;
                    let channel_layout = match aac_vs.format.nChannels {
                        1 => ChannelLayout::MONO,
                        2 => ChannelLayout::STEREO,
                        _ => return Err(Unknown.into())
                    };
                    let (frame, silent_frame) = create_audio_frames(sample, AAC_FRAME_SIZE, channel_layout);
                    let encoder = new_audio_encoder_aac(enc, codec, aac_vs.format.nSamplesPerSec as i32, channel_layout, sample)?;
                    let parameters = Parameters::from(&encoder);
                    let arc_ring_buffer = create_ring_buffer(aac_vs.format.nSamplesPerSec);
                    let recorder = AudioRecorder::new(arc_ring_buffer.clone(), aac_vs, encoder, frame, silent_frame);
                    Recorder::from_recorder(recorder, arc_ring_buffer, parameters)
                }
                AudioCodec::Test => {
                    todo!()
                }
            }
        }
    };

    Ok(idk)
}