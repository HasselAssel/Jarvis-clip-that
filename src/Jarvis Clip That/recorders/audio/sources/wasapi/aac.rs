use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use ffmpeg_next::encoder::audio::Encoder;
use ffmpeg_next::frame::Audio;
use windows::Win32::Media::Audio::{IAudioCaptureClient, WAVEFORMATEX};
use crate::recorders::audio::sources::wasapi::traits::WasapiEncoderCtx;
use crate::recorders::frame::copy_into_audio_frame;
use crate::recorders::traits::send_frame_and_receive_packets;
use crate::ring_buffer::traits::PacketRingBuffer;

pub struct AacContext;

pub const AAC_FRAME_SIZE: usize = 1024;

impl WasapiEncoderCtx for AacContext {
    fn process_audio<PRB: PacketRingBuffer>(&mut self, ring_buffer: &Arc<Mutex<PRB>>, mut encoder: &mut Encoder, mut frame: &mut Audio, silent_frame: &mut Audio, packet_length: u32, data: *mut u8, qpc_pos: u64, start_time: i64, frequency: i64, format: &WAVEFORMATEX, pts_counter: &mut i64, mut audio_buffer: &mut VecDeque<u8>, capture_client: &IAudioCaptureClient) {
        if packet_length > 0 {
            let new_pts = ((qpc_pos as i64 - start_time).max(0) as u64 * format.nSamplesPerSec as u64 / frequency as u64) as i64;
            let diff = (new_pts - *pts_counter).max(0);
            if diff >= AAC_FRAME_SIZE as i64 {
                flush_and_silence(&mut audio_buffer, diff, pts_counter, frame, silent_frame, format, ring_buffer, encoder);
            }

            let buffer = unsafe {std::slice::from_raw_parts(
                data as *const u8,
                packet_length as usize * format.nBlockAlign as usize,
            )};

            assert_eq!(buffer.len() % 8, 0);
            audio_buffer.extend(buffer);

            unsafe {capture_client.ReleaseBuffer(packet_length).unwrap(); };
        }

        let size = AAC_FRAME_SIZE * format.nBlockAlign as usize;
        while audio_buffer.len() >= size {
            let buffer: Vec<u8> = audio_buffer.drain(..size).collect();
            let sample_frames = AAC_FRAME_SIZE;


            unsafe { copy_into_audio_frame(&mut frame, buffer); }
            frame.set_pts(Some(*pts_counter));
            send_frame_and_receive_packets(&ring_buffer, &mut encoder, &frame, AAC_FRAME_SIZE as i64).unwrap();

            *pts_counter += sample_frames as i64;
        }
    }
}

fn flush_and_silence<PRB: PacketRingBuffer>(vec_to_be_flushed: &mut VecDeque<u8>, mut frames_of_silence: i64, start_pts: &mut i64, mut frame: &mut Audio, silent_frame: &mut Audio, format: &WAVEFORMATEX, ring_buffer: &Arc<Mutex<PRB>>, mut encoder: &mut Encoder) {
    let frame_size: usize = frame.samples();
    // flush
    let mut buffer = vec![0; frame_size * format.nBlockAlign as usize];
    assert!(buffer.len() >= vec_to_be_flushed.len());
    let flushed_pts = (buffer.len() - vec_to_be_flushed.len()) / format.nBlockAlign as usize;
    assert_eq!(flushed_pts * format.nBlockAlign as usize, buffer.len() - vec_to_be_flushed.len());

    let (first, second) = vec_to_be_flushed.as_slices();
    assert_eq!(first.len() + second.len(), vec_to_be_flushed.len());
    buffer[..first.len()].copy_from_slice(first);
    buffer[first.len()..vec_to_be_flushed.len()].copy_from_slice(second);
    frames_of_silence -= flushed_pts as i64;

    unsafe { copy_into_audio_frame(&mut frame, buffer); }
    frame.set_pts(Some(*start_pts));
    send_frame_and_receive_packets(&ring_buffer, &mut encoder, &frame, frame_size as i64).unwrap();
    *start_pts += frame_size as i64;

    // empty frames
    let whole_silent_frames = frames_of_silence / frame_size as i64;

    for _ in 0..whole_silent_frames {
        silent_frame.set_pts(Some(*start_pts));
        send_frame_and_receive_packets(&ring_buffer, &mut encoder, &silent_frame, frame_size as i64).unwrap();
        *start_pts += frame_size as i64;
    }
}