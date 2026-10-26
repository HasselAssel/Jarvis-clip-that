use anyhow::Result;
use ffmpeg_next::format::Pixel;
use ffmpeg_next::frame::Video;
use ffmpeg_next::software::scaling::{Context, Flags};

pub struct VideoConverter {
    scaler: Option<Context>,

    in_dim: (u32, u32),
    in_format: Pixel,

    out_format: Pixel,
}

unsafe impl Send for VideoConverter {}

impl VideoConverter {
    pub fn new(out_format: Pixel) -> Self {        
        Self {
            scaler: None,
            in_dim: (0, 0),
            in_format: Pixel::None,
            out_format
        }
    }

    pub fn convert(&mut self, input: &Video) -> Result<Video> {
        let in_format = input.format();
        let in_dim = (input.width(), input.height());

        if self.scaler.is_none() || self.in_dim != in_dim || self.in_format != in_format {
            self.scaler = Some(Context::get(
                in_format,
                in_dim.0,
                in_dim.1,
                self.out_format,
                in_dim.0,
                in_dim.1,
                Flags::FAST_BILINEAR,
            )?);
            self.in_dim = in_dim;
            self.in_format = in_format;
        }

        let mut output = Video::empty();
        let scaler = self.scaler.as_mut().unwrap();
        scaler.run(input, &mut output)?;

        output.set_pts(input.pts());

        Ok(output)
    }
}