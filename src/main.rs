use crate::capturer::clipper::Clipper;

mod capturer;

fn main() {
    ffmpeg_next::init().unwrap();
    let clipper= Clipper::new(30, 1280, 720, 120);

    clipper.start();
}