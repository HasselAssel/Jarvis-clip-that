use std::collections::VecDeque;

use anyhow::{anyhow, Result};
use ffmpeg_next::{ChannelLayout, Packet};
use ffmpeg_next::codec::{codec, Flags};
use ffmpeg_next::codec::encoder;
use ffmpeg_next::format::Sample;
use ffmpeg_next::util::format::sample;
use ffmpeg_next::util::frame::audio;
use windows::Win32::Media::Audio::WAVEFORMATEXTENSIBLE;
use windows::Win32::Media::KernelStreaming::KSDATAFORMAT_SUBTYPE_PCM;
use windows::Win32::Media::Multimedia::KSDATAFORMAT_SUBTYPE_IEEE_FLOAT;

use crate::recorders::traits::Encoder;
use crate::recorders::windows::wasapi::env::EnvWasapi;

pub struct EncoderWasapi {
    encoder: encoder::audio::Encoder,

    frame: audio::Audio,
    silent_frame: audio::Audio,

    queue: VecDeque<u8>,

    expected_device_pos: u64,
}

impl EncoderWasapi {
    pub fn new(
        enc: encoder::audio::Audio,
        codec: codec::Codec,
        frame_size: usize,
        env: &<Self as Encoder>::Env<'_>,
    ) -> Result<Self> {
        let channel_layout = match env.format.Format.nChannels {
            1 => ChannelLayout::MONO,
            2 => ChannelLayout::STEREO,
            _ => return Err(anyhow!("unsupported channel layout.")),
        };
        let sample = get_sample(&env.format);
        let encoder = new_audio_encoder_aac(
            enc,
            codec,
            env.format.Format.nSamplesPerSec as i32,
            channel_layout,
            sample,
        )?;

        let frame = audio::Audio::new(sample, frame_size, channel_layout);
        let mut silent_frame = audio::Audio::new(sample, frame_size, channel_layout);
        copy_packed_audio_into_frame(&mut silent_frame, &vec![0; frame_size]);

        Ok(Self {
            encoder,

            frame,
            silent_frame,

            queue: VecDeque::new(),
            expected_device_pos: 0,
        })
    }
}

impl Encoder for EncoderWasapi {
    type Env<'e> = EnvWasapi;
    type Input<'i> = (&'i [u8], u64);
    type Output<'o> = Vec<Packet>;

    fn encode(&mut self, input: Self::Input<'_>, env: &Self::Env<'_>) -> Result<Self::Output<'_>> {
        todo!()
    }
}

pub fn new_audio_encoder_aac(
    mut enc: encoder::audio::Audio,
    codec: codec::Codec,
    rate: i32,
    channel_layout: ChannelLayout,
    sample: Sample,
) -> Result<encoder::audio::Encoder> {
    enc.set_rate(rate);
    enc.set_channel_layout(channel_layout);
    enc.set_format(sample);
    enc.set_time_base((1, rate));
    enc.set_flags(Flags::GLOBAL_HEADER);

    let audio_encoder = enc.open_as(codec).unwrap();
    Ok(audio_encoder)
}

fn get_sample(format: &WAVEFORMATEXTENSIBLE) -> Sample {
    let bits = format.Format.wBitsPerSample;

    if format.SubFormat == KSDATAFORMAT_SUBTYPE_PCM {
        match bits {
            8 => Sample::U8(sample::Type::Packed),
            16 => Sample::I16(sample::Type::Packed),
            32 => Sample::I32(sample::Type::Packed),
            64 => Sample::I64(sample::Type::Packed),
            _ => Sample::None,
        }
    } else if format.SubFormat == KSDATAFORMAT_SUBTYPE_IEEE_FLOAT {
        match bits {
            32 => Sample::F32(sample::Type::Packed),
            64 => Sample::F64(sample::Type::Packed),
            _ => Sample::None,
        }
    } else {
        Sample::None
    }
}

pub fn copy_packed_audio_into_frame(
    frame: &mut audio::Audio,
    input: &[u8],
) -> usize {
    let bytes_per_sample = frame.format().bytes();
    let channels = frame.channels() as usize;
    let frame_capacity_samples = frame.samples();

    if bytes_per_sample == 0 || channels == 0 {
        return 0;
    }

    let bytes_per_audio_sample = bytes_per_sample * channels;

    let input_samples = input.len() / bytes_per_audio_sample;

    let samples_to_copy = input_samples.min(frame_capacity_samples);

    let bytes_to_consume = samples_to_copy * bytes_per_audio_sample;

    if frame.format().is_packed() {
        // Packed: direct copy
        let dst = frame.data_mut(0);

        dst[..bytes_to_consume]
            .copy_from_slice(&input[..bytes_to_consume]);
    } else {
        // Planar / Interleaved
        for channel in 0..channels {
            let dst = frame.data_mut(channel);

            for sample in 0..samples_to_copy {
                let src_start =
                    (sample * channels + channel) * bytes_per_sample;

                let dst_start =
                    sample * bytes_per_sample;

                dst[dst_start..dst_start + bytes_per_sample]
                    .copy_from_slice(
                        &input[src_start..src_start + bytes_per_sample]
                    );
            }
        }
    }

    bytes_to_consume
}