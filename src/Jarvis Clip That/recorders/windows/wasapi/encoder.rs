use anyhow::{anyhow, Result};
use ffmpeg_next::{ChannelLayout, Packet};
use ffmpeg_next::codec::{codec, Flags, Parameters};
use ffmpeg_next::codec::encoder;
use ffmpeg_next::format::Sample;
use ffmpeg_next::util::format::sample;
use ffmpeg_next::util::frame::audio;
use windows::Win32::Media::Audio::WAVEFORMATEXTENSIBLE;
use windows::Win32::Media::KernelStreaming::KSDATAFORMAT_SUBTYPE_PCM;
use windows::Win32::Media::Multimedia::KSDATAFORMAT_SUBTYPE_IEEE_FLOAT;

use crate::recorders::traits::Encoder;
use crate::recorders::windows::wasapi::env::EnvWasapi;
use crate::recorders::windows::wasapi::sources::WasApiAudioData;

pub struct EncoderWasapi {
    encoder: encoder::audio::Encoder,

    frame: audio::Audio,
    silent_frame: audio::Audio,
    frame_bytes: usize,

    pts_counter: i64,

    frame_queue_buffer: Box<[u8]>,
    frame_queue_buffer_pos: usize,

    expected_device_pos: Option<u64>,
}

impl EncoderWasapi {
    pub fn new(
        enc: encoder::audio::Audio,
        codec: codec::Codec,
        frame_size: usize,
        output_sample: Sample,
        env: &<Self as Encoder>::Env<'_>,
    ) -> Result<Self> {
        let channel_layout = match env.format.Format.nChannels {
            1 => ChannelLayout::MONO,
            2 => ChannelLayout::STEREO,
            _ => return Err(anyhow!("unsupported channel layout.")),
        };
        let input_sample = get_sample(&env.format);

        if input_sample.bytes() != output_sample.bytes() {
            return Err(anyhow!("Sample conversion not supported rn."));
        }

        let encoder = new_audio_encoder_aac(
            enc,
            codec,
            env.format.Format.nSamplesPerSec as i32,
            channel_layout,
            output_sample,
        )?;

        let frame = audio::Audio::new(output_sample, frame_size, channel_layout);
        let frame_bytes = frame.samples() * frame.channels() as usize * frame.format().bytes();

        let mut silent_frame = audio::Audio::new(output_sample, frame_size, channel_layout);
        copy_packed_audio_into_frame(&mut silent_frame, &vec![0; frame_bytes]);


        Ok(Self {
            encoder,

            frame,
            silent_frame,
            frame_bytes,

            pts_counter: 0,

            frame_queue_buffer: vec![0; frame_bytes].into_boxed_slice(),
            frame_queue_buffer_pos: 0,

            expected_device_pos: None,
        })
    }

    pub fn get_params(&self) -> Parameters {
        Parameters::from(&self.encoder)
    }

    fn encode_frame(
        encoder: &mut encoder::audio::Encoder,
        pts_counter: &mut i64,
        frame: &mut audio::Audio,
    ) -> Result<()> {
        frame.set_pts(Some(*pts_counter));
        encoder.send_frame(frame)?;

        *pts_counter += frame.samples() as i64;

        Ok(())
    }
}

impl Encoder for EncoderWasapi {
    type Env<'e> = EnvWasapi;
    type Input<'i> = WasApiAudioData<'i>;
    type Output = Vec<Packet>;

    fn encode(&mut self, input: Self::Input<'_>, env: &Self::Env<'_>) -> Result<Self::Output> {
        let mut packets = Vec::new();
        let mut packet = Packet::empty();

        let expected = *self.expected_device_pos.get_or_insert(input.device_pos);
        let missing_samples = input.device_pos.saturating_sub(expected);
        let block_align = env.format.Format.nBlockAlign as usize;

        // silence

        let mut missing_bytes = missing_samples as usize * block_align;

        if missing_bytes > 0 {
            // remaining space in the current frame
            if self.frame_queue_buffer_pos > 0 {
                let available = self.frame_bytes - self.frame_queue_buffer_pos;
                let count = missing_bytes.min(available);

                let end = self.frame_queue_buffer_pos + count;

                self.frame_queue_buffer[self.frame_queue_buffer_pos..end]
                    .fill(0);

                self.frame_queue_buffer_pos = end;
                missing_bytes -= count;

                if self.frame_queue_buffer_pos == self.frame_bytes {
                    copy_packed_audio_into_frame(
                        &mut self.frame,
                        &self.frame_queue_buffer,
                    );

                    Self::encode_frame(&mut self.encoder, &mut self.pts_counter, &mut self.frame)?;
                    self.frame_queue_buffer_pos = 0;

                    while self.encoder.receive_packet(&mut packet).is_ok() {
                        packets.push(std::mem::replace(
                            &mut packet,
                            Packet::empty(),
                        ));
                    }
                }
            }

            let full_frames = missing_bytes / self.frame_bytes;
            missing_bytes %= self.frame_bytes;

            for _ in 0..full_frames {
                Self::encode_frame(&mut self.encoder, &mut self.pts_counter, &mut self.silent_frame)?;

                while self.encoder.receive_packet(&mut packet).is_ok() {
                    packets.push(std::mem::replace(
                        &mut packet,
                        Packet::empty(),
                    ));
                }
            }

            if missing_bytes > 0 {
                self.frame_queue_buffer[..missing_bytes].fill(0);
                self.frame_queue_buffer_pos = missing_bytes;
            }
        }

        // audio

        let mut remaining_data = input.data;

        while !remaining_data.is_empty() {
            let available = self.frame_bytes - self.frame_queue_buffer_pos;
            let count = remaining_data.len().min(available);

            let end = self.frame_queue_buffer_pos + count;

            self.frame_queue_buffer[self.frame_queue_buffer_pos..end]
                .copy_from_slice(&remaining_data[..count]);

            self.frame_queue_buffer_pos = end;
            remaining_data = &remaining_data[count..];

            if self.frame_queue_buffer_pos == self.frame_bytes {
                copy_packed_audio_into_frame(
                    &mut self.frame,
                    &self.frame_queue_buffer,
                );

                Self::encode_frame(&mut self.encoder, &mut self.pts_counter, &mut self.frame)?;
                self.frame_queue_buffer_pos = 0;

                while self.encoder.receive_packet(&mut packet).is_ok() {
                    packets.push(std::mem::replace(
                        &mut packet,
                        Packet::empty(),
                    ));
                }
            }
        }

        self.expected_device_pos = Some(input.device_pos + input.packet_length as u64);

        Ok(packets)
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

    let sub_format = format.SubFormat;
    if sub_format == KSDATAFORMAT_SUBTYPE_PCM {
        match bits {
            8 => Sample::U8(sample::Type::Packed),
            16 => Sample::I16(sample::Type::Packed),
            32 => Sample::I32(sample::Type::Packed),
            64 => Sample::I64(sample::Type::Packed),
            _ => Sample::None,
        }
    } else if sub_format == KSDATAFORMAT_SUBTYPE_IEEE_FLOAT {
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
        let dst = frame.data_mut(0);

        dst[..bytes_to_consume]
            .copy_from_slice(&input[..bytes_to_consume]);
    } else { // planar
        unsafe {
            let raw = frame.as_mut_ptr();
            let plane_size = (*raw).linesize[0] as usize;

            for channel in 0..channels {
                let plane_ptr = *(*raw).extended_data.add(channel);

                assert!(!plane_ptr.is_null());

                let dst = std::slice::from_raw_parts_mut(
                    plane_ptr,
                    plane_size,
                );

                assert!(
                    dst.len() >= samples_to_copy * bytes_per_sample
                );

                for sample in 0..samples_to_copy {
                    let src_start =
                        (sample * channels + channel) * bytes_per_sample;

                    let dst_start = sample * bytes_per_sample;

                    dst[dst_start..dst_start + bytes_per_sample]
                        .copy_from_slice(
                            &input[src_start..src_start + bytes_per_sample]
                        );
                }
            }
        }
    }

    bytes_to_consume
}