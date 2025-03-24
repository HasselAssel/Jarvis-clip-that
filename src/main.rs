use std::time::Instant;

mod screenshot_winapi;
mod video;
mod screenshot_duplicationapi;


fn main() {
    //capture_one();
    //capture_multiple();
    //capture_multiple_to_h264();
    directx_try().expect("TODO: panic message");
}



fn directx_try() -> Result<(), windows::core::Error> {
    unsafe {
        // Capture the desktop and obtain the context and staging texture.
        let v = screenshot_duplicationapi::screenshot_duplication_api_main()?;
        // Save the staging texture as a PNG file.
        //screenshot_duplicationapi::save_texture_to_png(&v.0, &v.1, "out/screenshot.png").expect("TODO: panic message");
        //println!("Screenshot saved to screenshot.png");
    }
    Ok(())
}

























fn capture_one() {
    let mut start_time;
    let mut end_time;

    start_time = Instant::now();
    let a = screenshot_winapi::capture_screen();
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
    let a = screenshot_winapi::capture_screens(10, 5);
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
    let a = screenshot_winapi::capture_screens(100, fps);
    println!("{:?}", a);
    end_time = Instant::now(); println!("Capturing Time: {:?}", end_time - start_time);


    start_time = Instant::now();
    if let Ok((screenshots, width, height)) = &a {
        video::hbitmaps_to_h264(screenshots, fps).expect("TODO: panic message");;
    }
    end_time = Instant::now(); println!("Converting Time: {:?}", end_time - start_time);
}