use std::time::Instant;

mod screenshot;
mod video;


fn main() {
    //capture_one();
    //capture_multiple();
    capture_multiple_to_h264();
}

fn capture_one() {
    let mut start_time;
    let mut end_time;

    start_time = Instant::now();
    let a = screenshot::capture_screen();
    println!("{:?}", a);
    end_time = Instant::now(); println!("Capturing Time: {:?}", end_time - start_time);


    start_time = Instant::now();
    if let Ok((screenshot, width, height)) = &a {
        video::hbitmap_to_png(*screenshot, "out/screen.png").expect("TODO: panic message");
    }
    end_time = Instant::now(); println!("Saving Time: {:?}", end_time - start_time);
}

fn capture_multiple() {
    let mut start_time;
    let mut end_time;

    start_time = Instant::now();
    let a = screenshot::capture_screens(10, 5);
    println!("{:?}", a);
    end_time = Instant::now(); println!("Capturing Time: {:?}", end_time - start_time);


    start_time = Instant::now();
    if let Ok((screenshots, width, height)) = &a {
        for i in 0..screenshots.len() {
            video::hbitmap_to_png(screenshots[i], format!("out/{}.png", i).as_str()).expect("TODO: panic message");
        }
    }
    end_time = Instant::now(); println!("Saving Time: {:?}", end_time - start_time);
}

fn capture_multiple_to_h264() {
    let fps: u32 = 5;

    let mut start_time;
    let mut end_time;

    start_time = Instant::now();
    let a = screenshot::capture_screens(100, fps);
    println!("{:?}", a);
    end_time = Instant::now(); println!("Capturing Time: {:?}", end_time - start_time);


    start_time = Instant::now();
    if let Ok((screenshots, width, height)) = &a {
        video::hbitmaps_to_h264(screenshots, fps).expect("TODO: panic message");;
    }
    end_time = Instant::now(); println!("Converting Time: {:?}", end_time - start_time);
}



struct TimeTracker {
    last_time: Instant
}

impl TimeTracker {
    fn time_since_last_marker(&mut self) {
        let new_time = Instant::now();
        println!("Tracked Time: {:?}", new_time - self.last_time);
        self.last_time = new_time;
    }

    fn new() -> Self{
        Self {
            last_time: Instant::now()
        }
    }
}