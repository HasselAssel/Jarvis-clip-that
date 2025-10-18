use crate::media_playback::MediaPlayback;
use std::sync::mpsc;
use std::sync::mpsc::{Receiver, Sender};
use std::time::Instant;

use eframe::egui;
use eframe::egui::{Color32, ColorImage, TextureOptions};
use rodio::{OutputStream, Sink};
use rodio::buffer::SamplesBuffer;

use crate::media::Media;
use crate::decoders::{DecodedFrame};

mod media_playback;
mod media;
mod decoders;
mod hw_decoding;

struct MyApp {
    color_image: ColorImage,
    texture: Option<egui::TextureHandle>,

    receiver: Receiver<DecodedFrame>,
    test_audio_sender: Sender<SamplesBuffer<f32>>,
}

impl MyApp {
    fn new(receiver: Receiver<DecodedFrame>, test_audio_sender: Sender<SamplesBuffer<f32>>) -> Self {
        Self {
            color_image: Default::default(),
            texture: None,
            receiver,
            test_audio_sender,
        }
    }
}

impl eframe::App for MyApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if let Ok(decoded_frame) = self.receiver.try_recv() {
            if let Some((buffer, size)) = decoded_frame.video {
                let instant = Instant::now();
                if self.color_image.size == size {
                    let instant = Instant::now();
                    for (i, pixel) in buffer.chunks_exact(3).enumerate() {
                        self.color_image.pixels[i] = Color32::from_rgb(pixel[0], pixel[1], pixel[2]);
                    }
                    println!("weiß net set: {:?}", instant.elapsed());
                } else {
                    self.color_image = ColorImage::from_rgb(size, &buffer);
                }
                println!("idk set: {:?}", instant.elapsed());


                if let Some(ref mut tex) = self.texture {
                    let instant = Instant::now();
                    tex.set(self.color_image.clone(), TextureOptions::default());
                    println!("tex set: {:?}", instant.elapsed());
                } else {
                    self.texture = Some(ctx.load_texture(
                        "video_frame",
                        self.color_image.clone(),
                        TextureOptions::default(),
                    ));
                }
            }
            if let Some(sample) = decoded_frame.audio {
                self.test_audio_sender.send(sample).unwrap();
            }
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            if let Some(texture) = &self.texture {
                ui.image((texture.id(), egui::vec2(1000.0, texture.size()[1] as f32 * 1000.0 / texture.size()[0] as f32)));
            }
        });
    }
}

fn play_audio(receiver: Receiver<SamplesBuffer<f32>>) {
    let (_stream, handle) = OutputStream::try_default().unwrap();
    let sink = Sink::try_new(&handle).unwrap();
    tokio::spawn(async move {
        while let Ok(sample) = receiver.recv() {
            sink.append(sample);
        }
        println!("i am out");
        sink.sleep_until_end();
    });
}

async fn start(frame_sender: Sender<DecodedFrame>) {
    let mut media = Media::open_file("out/Chat Clip That_20250818_004213.mp4");
    let mut media_player = MediaPlayback::new(&mut media, frame_sender);
    media_player.play().await;
}

fn main() -> Result<(), eframe::Error> {
    let (tx, receiver) = mpsc::channel();
    let (sender, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        runtime.block_on(async move {
            play_audio(rx);
            start(tx).await;
        });
    });


    let options = eframe::NativeOptions::default();
    eframe::run_native(
        "My App",
        options,
        Box::new(|_cc| Ok(Box::new(MyApp::new(receiver, sender)))),
    )
}