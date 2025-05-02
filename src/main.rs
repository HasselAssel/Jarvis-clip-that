use crate::capturer::clipper::Clipper;

mod capturer;

fn main() {
    ffmpeg_next::init().unwrap();
    let clipper= Clipper::new(30, 1500, 1000, 120);

    clipper.start();
}