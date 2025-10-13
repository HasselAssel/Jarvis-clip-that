use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use chrono::Local;
use ffmpeg_next::codec::Parameters;
use ffmpeg_next::format::context;
use crate::ring_buffer::traits::PacketRingBuffer;
use crate::types::{Packet, Result};

pub struct Save {
    file_name: String,

    o_ctx: context::Output,
    streams: Vec<(Vec<Packet>, (i32, i32))>,
}

impl Save {
    fn new(file_name: String) -> Self {
        let o_ctx = ffmpeg_next::format::output_as(&file_name, "mp4").unwrap();
        let streams = Vec::new();
        Self {
            file_name,
            o_ctx,
            streams,
        }
    }

    pub fn add_stream<PRB: PacketRingBuffer>(&mut self, ring_buffer: &Arc<Mutex<PRB>>, parameters: &Parameters, is_video_else_audio: bool) -> Result<()> {
        let mut ost = &mut self.o_ctx.add_stream(parameters.id())?;
        ost.set_parameters(parameters.clone());

        let tb = match is_video_else_audio {
            true => { (unsafe { *parameters.as_ptr() }.framerate.den, unsafe { *parameters.as_ptr() }.framerate.num) }
            false => { (1, unsafe { *parameters.as_ptr() }.sample_rate) }
        };
        ost.set_time_base(tb);


        let ring_buffer = ring_buffer.lock().unwrap();
        let packets = ring_buffer.copy_out(None);

        self.streams.push((packets, tb));

        Ok(())
    }

    pub fn finalize_and_save(mut self) -> Result<()> {
        self.o_ctx.write_header().unwrap();

        let time_bases = self.o_ctx.streams().map(|stream| stream.time_base()).collect::<Vec<_>>().into_iter();

        for (i, (time_base, (packets, tb))) in time_bases.zip(self.streams.into_iter()).enumerate() {
            for mut packet in packets {
                packet.set_stream(i);
                packet.rescale_ts(tb, time_base);
                packet.write_interleaved(&mut self.o_ctx)?;
            }
        }

        self.o_ctx.write_trailer().unwrap();

        Ok(())
    }
}

pub struct SaverEnv {
    out_dir_path: String,
    base_file_name: String,

    sound_dir: String,
    preferred_sound_file_name: Option<String>,
}

impl SaverEnv {
    pub fn new<S: Into<String>>(out_dir_path: S, base_file_name: S) -> Self {
        let out_dir_path = out_dir_path.into();
        let base_file_name = base_file_name.into();

        let sound_dir = "sounds".into();
        let preferred_sound_file_name = None;

        let output_dir = PathBuf::from(&out_dir_path);
        if !output_dir.exists() {
            std::fs::create_dir_all(&output_dir).expect("Failed to create output directory");
        }

        Self {
            out_dir_path,
            sound_dir,
            preferred_sound_file_name,
            base_file_name,
        }
    }

    pub fn new_save<S: Into<String>>(&self, file_name: Option<S>) -> Save {
        let file_name = match file_name {
            None => { self.get_file_name("mp4") }
            Some(file_name) => { file_name.into() }
        };;

        Save::new(file_name)
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