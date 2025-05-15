use crate::capturer::clipper::Clipper;

mod capturer;

fn main() {
    ffmpeg_next::init().unwrap();
    //ffmpeg_next::log::set_level(ffmpeg_next::log::Level::Debug);
    //let clipper= Clipper::new(30, 1280, 720, 120);
    let clipper= Clipper::new(30, 2560, 1440, 120);

    clipper.start();
}