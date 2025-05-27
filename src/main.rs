mod capturer;
mod com;

use crate::capturer::capture::capturer::Capturer;

fn main() {
    ffmpeg_next::init().unwrap();
    ffmpeg_next::log::set_level(ffmpeg_next::log::Level::Debug);
    let mut cap = Capturer::new();
    cap.start_capturing();
    //let clipper= Clipper::new(30, 1280, 720, 120);
    //let clipper= Clipper::new(30, 2560, 1440, 20);

    //clipper.start();
}