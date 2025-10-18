use std::collections::{HashMap, VecDeque};
use std::fmt::Debug;
use std::sync::mpsc::Sender;
use std::time::{Duration, Instant};

use ffmpeg_next::decoder;
use ffmpeg_next::media::Type;
use ffmpeg_next::packet::Ref;

use crate::media::Media;
use crate::decoders::DecodedFrame;
use crate::decoders::Decoder;
use crate::hw_decoding::idk_yet;

pub struct MediaPlayback<'a> {
    media: &'a mut Media,
    pos: u32,
    is_playing: bool,
    frame_buffer: VecDeque<DecodedFrame>,
    frame_sender: Sender<DecodedFrame>,
    decoders: HashMap<usize, Box<dyn Decoder>>,
}

impl<'a> MediaPlayback<'a> {
    pub fn new(media: &'a mut Media, frame_sender: Sender<DecodedFrame>) -> Self {
        let decoders = media.streams
            .iter()
            .filter_map(|(index, stream)| {
                let codec = decoder::find(stream.parameters.id())?;
                let mut ctx = ffmpeg_next::codec::context::Context::new_with_codec(codec);
                ctx.set_parameters(stream.parameters.clone()).ok()?;
                let decoder: Box<dyn Decoder> = match stream.parameters.medium() {
                    Type::Video => {
                        Box::new(ctx.decoder().video().ok()?)
                    }
                    Type::Audio => {
                        Box::new(ctx.decoder().audio().ok()?)
                    }
                    _ => todo!()
                };

                Some((*index, decoder))
            }).collect();

        Self {
            media,
            pos: 0,
            is_playing: true,
            frame_buffer: Default::default(),
            frame_sender,
            decoders,
        }
    }

    pub async fn play(&mut self) {
        self.is_playing = true;


        let mut packet_iter = self.media.ictx.packets();
        while self.is_playing {
            if let Some((stream, packet)) = packet_iter.next() {
                //println!("index: {}, {}, {}, {}", stream.index(), packet.pts().unwrap(), stream.time_base(), packet.pts().unwrap() as f64 * stream.time_base().0 as f64 / stream.time_base().1 as f64);
                if let Some(decoder) = self.decoders.get_mut(&stream.index()) {
                    let intsant = Instant::now();
                    let (buffer, width, height) = unsafe { idk_yet(decoder.get_codec_ctx(), packet.as_ptr() as _, )};
                    println!("TIME: {:?}", intsant.elapsed());
                    image::save_buffer("out/frame.png", &buffer, width, height, image::ColorType::Rgb8).unwrap();
                    std::process::exit(0);

                        for decoded_frame in decoder.process_packet(&packet) {
                        //println!("Sending!");
                        self.frame_sender.send(decoded_frame).unwrap()
                    }
                }
            } else {
                break;
            }
        }
    }


    pub async fn play2(&mut self) {
        /*let video_buffer = VecDeque::new();
        let audio_buffer = VecDeque::new();

        let video_stream_ts = 0;
        let audio_stream_ts = 0;

        let global_packet_index = 0;

        let mut packet_iter = self.media.ictx.packets();*/

    }

    pub async fn stop(&self) {
        todo!()
    }

    fn decode_next(&self) -> DecodedFrame {
        todo!()
    }
}