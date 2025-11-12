use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use chrono::Local;
use ffmpeg_next::codec::Parameters;
use ffmpeg_next::format::context;
use rodio::Decoder;
use crate::debug_println;

use crate::ring_buffer::traits::PacketRingBuffer;
use crate::types::{Packet, Result};

pub struct Save {
    o_ctx: context::Output,
    streams: Vec<(Vec<Packet>, (i32, i32))>,

    save_sound_decoder: Option<Decoder<Cursor<Vec<u8>>>>,
}

impl Save {
    fn new(file_name: String, save_sound_file: Option<Vec<u8>>) -> Self {
        let o_ctx = ffmpeg_next::format::output_as(&file_name, "mp4").unwrap();
        let streams = Vec::new();

        let save_sound_decoder = save_sound_file.and_then(|save_sound_file| Some(Decoder::new(Cursor::new(save_sound_file)).ok()?));

        Self {
            o_ctx,
            streams,
            save_sound_decoder,
        }
    }

    pub fn add_stream<PRB: PacketRingBuffer>(&mut self, ring_buffer: &Arc<Mutex<PRB>>, parameters: &Parameters, is_video_else_audio: bool) -> Result<()> {
        let ost = &mut self.o_ctx.add_stream(parameters.id())?;
        ost.set_parameters(parameters.clone());

        let tb = match is_video_else_audio {
            true => { (unsafe { *parameters.as_ptr() }.framerate.den, unsafe { *parameters.as_ptr() }.framerate.num) }
            false => { (1, unsafe { *parameters.as_ptr() }.sample_rate) }
        };
        ost.set_time_base(tb);


        let ring_buffer = ring_buffer.lock().unwrap();
        let packets = ring_buffer.copy_out(None);
        drop(ring_buffer);

        let packets_pts: Vec<_> = packets.iter().map(|packet| packet.pts().unwrap()).collect();
        debug_println!("NEW STREAM PTS: {:?}", packets_pts);
        debug_println!("-------------------------------------------------------------------------------------------------------------------------------");

        self.streams.push((packets, tb));

        Ok(())
    }

    pub fn finalize_and_save(mut self) -> Result<()> {
        let min_pts_in_base_1_sec = self.streams.iter().filter_map(|(packets, tb)| if let Some(packet) = packets.first() {
            Some((packet, tb))
        } else {
            None
        }).filter_map(|(packet, tb)| if let Some(pts) = packet.pts() {
            Some(pts as f64 / (tb.1 as f64 / tb.0 as f64))
        } else {
            None
        }).reduce(f64::min).unwrap_or(0.0);

        debug_println!("min pts: {}", min_pts_in_base_1_sec);

        let _ = self.streams.iter_mut().for_each(|(packets, tb)| packets.iter_mut().for_each(|packet| packet.set_pts(packet.pts().map(|pts| (pts as f64 - (tb.1 as f64 / tb.0 as f64) * min_pts_in_base_1_sec) as i64))));

        self.o_ctx.write_header().unwrap();

        let time_bases = self.o_ctx.streams().map(|stream| stream.time_base()).collect::<Vec<_>>().into_iter();
        for (i, (time_base, (packets, tb))) in time_bases.zip(self.streams.into_iter()).enumerate() {
            let packets_pts: Vec<_> = packets.iter().map(|packet| packet.pts().unwrap()).collect();
            debug_println!("NEW STREAM PTS: {:?}", packets_pts);
            debug_println!("-------------------------------------------------------------------------------------------------------------------------------");

            for mut packet in packets {
                packet.set_stream(i);
                packet.rescale_ts(tb, time_base);
                packet.write_interleaved(&mut self.o_ctx)?;
            }
        }

        self.o_ctx.write_trailer().unwrap();


        if let Some(save_sound_decoder) = self.save_sound_decoder {
            std::thread::spawn(|| {
                let (_stream, handle) = rodio::OutputStream::try_default().unwrap();
                let sink = rodio::Sink::try_new(&handle).unwrap();
                sink.append(save_sound_decoder);
                sink.set_volume(0.1);
                sink.sleep_until_end()
            });
        }

        Ok(())
    }
}

#[derive(Clone)]
pub struct SaverEnv {
    out_dir_path: String,
    base_file_name: String,

    preferred_sound_file: Option<Vec<u8>>,
}

impl SaverEnv {
    pub fn new<S: Into<String>>(out_dir_path: S, base_file_name: S, preferred_sound_file_name: Option<&str>) -> Self {
        let out_dir_path = out_dir_path.into();
        let base_file_name = base_file_name.into();

        let preferred_sound_file = preferred_sound_file_name.and_then(|preferred_sound_file_name| Some(std::fs::read(preferred_sound_file_name).ok()?));

        let output_dir = PathBuf::from(&out_dir_path);
        if !output_dir.exists() {
            std::fs::create_dir_all(&output_dir).expect("Failed to create output directory");
        }

        Self {
            out_dir_path,
            base_file_name,
            preferred_sound_file,
        }
    }

    pub fn new_save<S: Into<String>>(&self, file_name: Option<S>) -> Save {
        let file_name = match file_name {
            None => { self.get_file_name("mp4") }
            Some(file_name) => { file_name.into() }
        };

        let save_sound_file = self.preferred_sound_file.as_ref().map(|preferred_sound_file| preferred_sound_file.clone());

        Save::new(file_name, save_sound_file)
    }

    fn get_file_name<S: Into<String>>(&self, extension: S) -> String {
        let extension = extension.into();

        let default_name = format!("{}/{}_{}", self.out_dir_path, self.base_file_name, Local::now().format("%Y%m%d_%H%M%S").to_string());

        let first_try = format!("{}.{}", default_name, extension);
        if !Path::new(&first_try).exists() {
            first_try
        } else {
            let mut found = None;
            for i in 1..=999 {
                let candidate = format!("{}_{:03}.{}", default_name, i, extension);
                if !Path::new(&candidate).exists() {
                    found = Some(candidate);
                    break;
                }
            }
            found.expect("All 999 filenames exist!")
        }
    }
}