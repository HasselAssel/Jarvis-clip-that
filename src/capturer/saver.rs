use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use chrono::Local;
use crate::capturer::ring_buffer::RingBuffer;

pub struct Saver {
    video_encoder: Arc<Mutex<ffmpeg_next::codec::encoder::Video>>,
    ring_buffer: Arc<Mutex<RingBuffer>>,

    out_dir_path: String,
    base_file_name: String,
    extension: String,
}

impl Saver {
    pub fn new<S: Into<String>>(video_encoder: Arc<Mutex<ffmpeg_next::codec::encoder::Video>>, ring_buffer: Arc<Mutex<RingBuffer>>, out_dir_path: S, base_file_name: S, extension: S) -> Self {
        let out_dir_path = out_dir_path.into();
        let base_file_name = base_file_name.into();
        let extension = extension.into();

        let output_dir = PathBuf::from(&out_dir_path);
        if !output_dir.exists() {
            std::fs::create_dir_all(&output_dir).expect("Failed to create output directory");
        }
        Self {
            video_encoder,
            ring_buffer,

            out_dir_path,
            base_file_name,
            extension,
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

        //let mut octx: ffmpeg_next::format::context::Output = ffmpeg_next::format::output_as(&output_path, "mp4")?;
        let mut octx: ffmpeg_next::format::context::Output = ffmpeg_next::format::output(&output_path)?;


        let ring_buffer = self.ring_buffer.lock().unwrap();
        let mut packets = ring_buffer.get_slice(min_requested_frames);
        drop(ring_buffer);

        let mut video_encoder_guard = self.video_encoder.lock().unwrap();
        let mut video_encoder = &mut *video_encoder_guard;
        let input_tb = video_encoder.time_base();
        let codec_id = video_encoder.codec().unwrap().id();

        let mut ost = octx.add_stream(ffmpeg_next::codec::encoder::find(codec_id))?;
        ost.set_parameters(&video_encoder);
        drop(video_encoder_guard);

        ost.set_time_base(input_tb);

        octx.write_header()?;

        let output_tb = octx.stream(0).unwrap().time_base();

        for mut pkt in packets {
            println!("PTS: {}", pkt.pts().unwrap());
            pkt.set_stream(0);
            pkt.rescale_ts(input_tb, output_tb);
            pkt.write_interleaved(&mut octx)?;
        }

        // 8. Write trailer and close
        octx.write_trailer()?;

        Ok(())
    }

    fn play_sound(&self, ) {
        // Get a default output stream and handle
        let (_stream, stream_handle) = rodio::OutputStream::try_default().unwrap();

        // Create a new sink (audio output)
        let sink = rodio::Sink::try_new(&stream_handle).unwrap();

        // Load and decode the audio file
        let file = std::fs::File::open("sound.mp3").unwrap(); // or .wav, .flac, etc.
        let source = rodio::Decoder::new(std::io::BufReader::new(file)).unwrap();

        // Play the sound
        sink.append(source);
    }
}