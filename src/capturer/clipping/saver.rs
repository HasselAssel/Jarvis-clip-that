use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::thread;
use chrono::Local;
use ffmpeg_next::codec::encoder::{Video, Audio};
use ffmpeg_next::{codec, Packet};
use rand::seq::IteratorRandom;
use crate::capturer::capture::recorder::{Recorder, RecorderCarrier};
use crate::capturer::capture::video_capturer::VideoCapturer;
use crate::capturer::error::IdkCustomErrorIGuess;
use crate::capturer::ring_buffer::{KeyFrameStartPacketWrapper, PacketRingBuffer, RingBuffer};
use crate::capturer::capture::recorder::VideoParams;

pub struct Saver {
    video_parameters: VideoParams,
    audio_parameters: VideoParams,

    out_dir_path: String,
    base_file_name: String,
    extension: String,

    sound_dir: String,
    preferred_sound_file_name: Option<String>,
}

impl Saver {
    pub fn new<S: Into<String>>(video_parameters: VideoParams, audio_parameters: VideoParams, out_dir_path: S, base_file_name: S, extension: S) -> Self {
        let out_dir_path = out_dir_path.into();
        let base_file_name = base_file_name.into();
        let extension = extension.into();

        let sound_dir = "sounds".into();
        let preferred_sound_file_name = None;


        let output_dir = PathBuf::from(&out_dir_path);
        if !output_dir.exists() {
            std::fs::create_dir_all(&output_dir).expect("Failed to create output directory");
        }
        Self {
            video_parameters,
            audio_parameters,

            out_dir_path,
            base_file_name,
            extension,

            sound_dir,
            preferred_sound_file_name,
        }
    }

    fn get_file_name(&self) -> String {
        let default_name = format!("{}/{}_{}", self.out_dir_path, self.base_file_name, Local::now().format("%Y%m%d_%H%M%S").to_string());
        if !Path::new(&format!("{}{}", default_name, self.extension)).exists() {
            format!("{}{}", default_name, self.extension)
        } else {
            let mut found = None;
            for i in 1..=999 {
                let candidate = format!("{}_{:03}{}", default_name, i, self.extension);
                if !Path::new(&candidate).exists() {
                    found = Some(candidate);
                    break;
                }
            }
            found.expect("All 999 filenames exist!")
        }
    }

    pub fn standard_save<P: PacketRingBuffer, R: Recorder<P>>(&self, video_capturers: RecorderCarrier<P, R>, audio_capturers: RecorderCarrier<P, R>, min_requested_frames: Option<i32>) -> Result<(), IdkCustomErrorIGuess> {
        let output_path = self.get_file_name();

        let mut octx: ffmpeg_next::format::context::Output = ffmpeg_next::format::output_as(&output_path, "mp4")?;


        let ring_buffer = video_capturers.ring_buffer.lock().unwrap();  // TODO: not just index 0!
        let mut video_packets = ring_buffer.copy_out(min_requested_frames);
        drop(ring_buffer);
        let offset = video_packets.iter().map(|a| a.pts().unwrap_or(i64::MAX)).min().unwrap_or(0);
        let max = video_packets.iter().map(|a| a.pts().unwrap_or(i64::MIN)).max().unwrap_or(0);
        println!("video: min: {}, max: {}", offset, max);
        Self::adjust_pts(&mut video_packets, offset);

        let ring_buffer = audio_capturers.ring_buffer.lock().unwrap();  // TODO: not just index 0!
        let mut audio_packets = ring_buffer.copy_out(min_requested_frames);
        drop(ring_buffer);
        let offset = ((offset - 1/*WEIRD BUT MAYBE GOOD*/) * 1600/*48000 / 30*/);// & 0x7FFFFFFFFFFFFC00i64; // TODO: NOT FINAL!!!!!!!!!!
        let _offset = audio_packets.iter().map(|a| a.pts().unwrap_or(i64::MAX)).min().unwrap_or(0);
        let max = audio_packets.iter().map(|a| a.pts().unwrap_or(i64::MIN)).max().unwrap_or(0);
        println!("audio: min: {}, max: {}, used offset: {}", _offset, max, offset);
        Self::adjust_pts(&mut audio_packets, offset);

        println!("{:?}", video_packets.iter().map(|p| p.pts().unwrap()).collect::<Vec<_>>());
        println!("{:?}", audio_packets.iter().map(|p| p.pts().unwrap()).collect::<Vec<_>>());



        let mut video_ost = octx.add_stream(codec::encoder::find(self.video_parameters.codec))?;
        let params = video_ost.parameters();
        video_ost.set_parameters(self.video_parameters.parameters.clone());
        video_ost.set_time_base(self.video_parameters.time_base);

        let mut audio_ost = octx.add_stream(codec::encoder::find(self.audio_parameters.codec))?;
        audio_ost.set_parameters(self.audio_parameters.parameters.clone());
        audio_ost.set_time_base(self.audio_parameters.time_base);



        octx.write_header()?;

        let output_tb_0 = octx.stream(0).unwrap().time_base();
        let output_tb_1 = octx.stream(1).unwrap().time_base();

        for mut pkt in video_packets {
            pkt.set_stream(0);
            pkt.rescale_ts(self.video_parameters.time_base, output_tb_0);
            pkt.write_interleaved(&mut octx)?;
        }

        for mut pkt in audio_packets {
            pkt.set_stream(1);
            pkt.rescale_ts(self.audio_parameters.time_base, output_tb_1);
            pkt.write_interleaved(&mut octx)?;
        }

        octx.write_trailer()?;

        let _ = self.play_sound();

        Ok(())
    }

    pub fn adjust_pts(packets: &mut [Packet], offset: i64) {
        packets.iter_mut().for_each(|a| {
            if let Some(pts) = a.pts() { a.set_pts(Some(pts - offset)); }
            if let Some(dts) = a.dts() { a.set_dts(Some(dts - offset)); }
        });
    }

    fn play_sound(&self) -> Result<(), std::io::ErrorKind> {
        let file_path = if let Some(file_name) = &self.preferred_sound_file_name {
            Path::new(&self.sound_dir).join(file_name)
        } else {
            let mp3_files = std::fs::read_dir(&self.sound_dir).ok().unwrap().filter_map(|entry| {
                let path = entry.ok().unwrap().path();
                if path.extension().and_then(|e| e.to_str()) == Some("mp3") {
                    Some(path)
                } else {
                    None
                }
            });

            mp3_files.choose(&mut rand::rng()).unwrap()
        };

        thread::spawn(move || {
            // Get a default output stream and handle
            let (_stream, stream_handle) = rodio::OutputStream::try_default().unwrap();

            // Create a new sink (audio output)
            let sink = rodio::Sink::try_new(&stream_handle).unwrap();

            // Load and decode the audio file
            let file = std::fs::File::open(file_path).unwrap();
            let source = rodio::Decoder::new(std::io::BufReader::new(file)).unwrap();

            // Play the sound
            sink.set_volume(0.05);
            sink.append(source);
            sink.sleep_until_end(); // Todo maybe different thread!! blocks main!
        });

        Ok(())
    }
}