use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::thread;
use chrono::Local;
use rand::seq::IteratorRandom;
use crate::capturer::ring_buffer::RingBuffer;

pub struct Saver {
    video_encoder: Arc<Mutex<ffmpeg_next::codec::encoder::Video>>,
    video_ring_buffer: Arc<Mutex<RingBuffer>>,
    audio_encoder: Arc<Mutex<ffmpeg_next::codec::encoder::Audio>>,
    audio_ring_buffer: Arc<Mutex<RingBuffer>>,

    out_dir_path: String,
    base_file_name: String,
    extension: String,

    sound_dir: String,
    preferred_sound_file_name: Option<String>,
}

impl Saver {
    pub fn new<S: Into<String>>(video_encoder: Arc<Mutex<ffmpeg_next::codec::encoder::Video>>, video_ring_buffer: Arc<Mutex<RingBuffer>>, audio_encoder: Arc<Mutex<ffmpeg_next::codec::encoder::Audio>>, audio_ring_buffer: Arc<Mutex<RingBuffer>>, out_dir_path: S, base_file_name: S, extension: S) -> Self {
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
            video_encoder,
            video_ring_buffer,
            audio_encoder,
            audio_ring_buffer,

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

    pub fn standard_save(&self, min_requested_frames: Option<i32>) -> std::result::Result<(), ffmpeg_next::Error> {
        let output_path = self.get_file_name();

        let mut octx: ffmpeg_next::format::context::Output = ffmpeg_next::format::output_as(&output_path, "mp4")?;
        //let mut octx: ffmpeg_next::format::context::Output = ffmpeg_next::format::output(&output_path)?;


        let ring_buffer = self.video_ring_buffer.lock().unwrap();
        let video_packets = ring_buffer.get_slice(min_requested_frames);
        drop(ring_buffer);

        /*let ring_buffer = self.audio_ring_buffer.lock().unwrap();
        let audio_packets = ring_buffer.get_slice(min_requested_frames);
        drop(ring_buffer);*/


        let mut video_encoder_guard = self.video_encoder.lock().unwrap();
        let video_encoder = &mut *video_encoder_guard;
        let video_input_tb = video_encoder.time_base();
        let video_codec_id = video_encoder.codec().unwrap().id();

        let mut video_ost = octx.add_stream(ffmpeg_next::codec::encoder::find(video_codec_id))?;
        video_ost.set_parameters(video_encoder);
        video_ost.set_time_base(video_input_tb);
        drop(video_encoder_guard);


        /*let mut audio_encoder_guard = self.audio_encoder.lock().unwrap();
        let audio_encoder = &mut *audio_encoder_guard;
        let audio_input_tb = audio_encoder.time_base();
        let audio_codec_id = audio_encoder.codec().unwrap().id();

        let mut audio_ost = octx.add_stream(ffmpeg_next::codec::encoder::find(audio_codec_id))?;
        audio_ost.set_parameters(audio_encoder);
        audio_ost.set_time_base(audio_input_tb);
        drop(audio_encoder_guard);*/



        octx.write_header()?;

        let output_tb_0 = octx.stream(0).unwrap().time_base();
        //let output_tb_1 = octx.stream(1).unwrap().time_base();

        for mut pkt in video_packets {
            pkt.set_stream(0);
            pkt.rescale_ts(video_input_tb, output_tb_0);
            pkt.write_interleaved(&mut octx)?;
        }

        /*for mut pkt in audio_packets {
            pkt.set_stream(1);
            pkt.rescale_ts(audio_input_tb, output_tb_1);
            pkt.write_interleaved(&mut octx)?;
        }*/

        octx.write_trailer()?;

        let _ = self.play_sound();

        Ok(())
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
            sink.sleep_until_end();
        });

        Ok(())
    }
}