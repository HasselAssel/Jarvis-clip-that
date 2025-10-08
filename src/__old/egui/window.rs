use std::time::{Duration, Instant};
use eframe::egui;
use ffmpeg_next as ffmpeg;
use ffmpeg_next::codec::traits::Decoder;

pub fn _main() -> eframe::Result {
    // Initialize FFmpeg
    ffmpeg::init().map_err(|e| format!("FFmpeg init failed: {}", e)).unwrap();

    // Create native options
    let options = eframe::NativeOptions {
        //initial_window_size: Some(egui::vec2(800.0, 600.0)),
        ..Default::default()
    };

    // Run the egui application
    eframe::run_native(
        "Video Player",
        options,
        Box::new(|cc| Ok(Box::new(VideoPlayerApp::new(cc)))),
    )
}

struct VideoPlayerApp {
    player: Option<VideoPlayer>,
    packets: Vec<ffmpeg::Packet>,
    status: String,
    is_playing: bool,
    playback_start: Option<Instant>,
}

impl VideoPlayerApp {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let mut app = Self {
            player: None,
            packets: Vec::new(),
            status: "Load a video file".to_string(),
            is_playing: false,
            playback_start: None,
        };
        app.load_demo_video();
        app
    }

    fn load_demo_video(&mut self) {
        // In a real app, you'd load actual video packets here
        // This is a simplified version with dummy packets
        self.status = "Loaded demo video".to_string();
        self.player = Some(VideoPlayer::new());
        self.packets = vec![ffmpeg::Packet::empty(); 100]; // Dummy packets
    }

    fn toggle_playback(&mut self) {
        self.is_playing = !self.is_playing;
        if self.is_playing {
            self.playback_start = Some(Instant::now());
            self.status = "Playing".to_string();
        } else {
            self.status = "Paused".to_string();
        }
    }
}

impl eframe::App for VideoPlayerApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Update playback time if playing
        if self.is_playing {
            ctx.request_repaint();
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("Video Player");

            // Control buttons
            ui.horizontal(|ui| {
                if ui.button(if self.is_playing { "⏸ Pause" } else { "▶ Play" }).clicked() {
                    self.toggle_playback();
                }

                if ui.button("⏹ Stop").clicked() {
                    self.is_playing = false;
                    self.playback_start = None;
                    self.status = "Stopped".to_string();
                }
            });

            ui.separator();
            ui.label(&self.status);

            // Video display area
            if let Some(player) = &mut self.player {
                let frame = if self.is_playing {
                    // Calculate current playback time
                    let elapsed = self.playback_start.unwrap().elapsed();
                    player.get_frame(elapsed, &self.packets)
                } else {
                    None
                };

                // Display video frame or placeholder
                if let Some((width, height, image)) = frame {
                    // Create texture if needed
                    let texture = ui.ctx().load_texture(
                        "video-frame",
                        egui::ColorImage::from_rgba_unmultiplied(
                            [width, height],
                            &image,
                        ),
                        Default::default(),
                    );

                    // Show the video frame
                    ui.image(&texture);
                } else {
                    // Show placeholder when paused or no frame
                    let size = egui::vec2(640.0, 480.0);
                    ui.painter().rect_filled(
                        egui::Rect::from_min_size(ui.cursor().min, size),
                        0.0,
                        egui::Color32::from_gray(20),
                    );
                    ui.label("No video frame");
                }
            }
        });
    }
}

struct VideoPlayer {
    //decoder: ffmpeg::codec::decoder::Video,
    scaler: ffmpeg::software::scaling::Context,
    last_frame: Option<egui::ColorImage>,
}

impl VideoPlayer {
    fn new() -> Self {
        // Setup dummy decoder (in real app you'd use actual codec parameters)
        let codec = ffmpeg::codec::decoder::find(ffmpeg::codec::Id::H264)
            .expect("Codec not found")
            .decoder().unwrap()
            .video()
            .expect("Not a video codec");

        // Setup scaler (RGB0 format for egui)
        let scaler = ffmpeg::software::scaling::Context::get(
            ffmpeg::format::Pixel::YUV420P,
            640,
            480,
            ffmpeg::format::Pixel::RGB8,
            640,
            480,
            ffmpeg::software::scaling::Flags::BILINEAR,
        )
            .expect("Failed to create scaler");

        Self {
            //decoder: codec,
            scaler,
            last_frame: None,
        }
    }

    fn get_frame(
        &mut self,
        elapsed: Duration,
        packets: &[ffmpeg::Packet],
    ) -> Option<(usize, usize, Vec<u8>)> {
        // Calculate current timestamp (simulated PTS handling)
        let current_pts = (elapsed.as_secs_f64() * 30f64) as u32; // ms

        // Find packet index based on timestamp
        //let packet_idx = (current_pts / 100.0) as usize % packets.len();

        // Simulate decoding process
        let width = 640;
        let height = 480;

        // Generate test pattern based on playback time
        let mut image = vec![0; width * height * 4];
        let time = elapsed.as_secs_f32();

        for y in 0..height {
            for x in 0..width {
                let i = (y * width + x) * 4;
                let dx = (x as f32 / width as f32 * 2.0 - 1.0);
                let dy = (y as f32 / height as f32 * 2.0 - 1.0);

                // Create moving color pattern
                image[i] = (((time * 0.1).sin() * 0.5 + 0.5) * 255.0) as u8;
                image[i + 1] = (((dx * time).sin() * 0.5 + 0.5) * 255.0) as u8;
                image[i + 2] = (((dy * time).cos() * 0.5 + 0.5) * 255.0) as u8;
                image[i + 3] = 255; // Alpha
            }
        }

        Some((width, height, image))
    }
}