use anyhow::Result;
use ffmpeg_next::channel_layout::ChannelLayout;
use ffmpeg_next::format::Pixel;
use ffmpeg_next::format::Sample;
use ffmpeg_next::frame::Video;
use ffmpeg_next::frame::Audio;
use ffmpeg_next::software::scaling;
use ffmpeg_next::software::resampling;
use ffmpeg_next::format::sample::Type;

pub struct VideoConverter {
    scaler: Option<scaling::Context>,

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
            self.scaler = Some(scaling::Context::get(
                in_format,
                in_dim.0,
                in_dim.1,
                self.out_format,
                in_dim.0,
                in_dim.1,
                scaling::Flags::FAST_BILINEAR,
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

pub struct AudioConverter {
    resampler: Option<resampling::Context>,

    in_format: Sample,
    in_channel_layout: ChannelLayout,
    in_rate: u32,
}

unsafe impl Send for AudioConverter {}

impl AudioConverter {
    pub fn new() -> Self {
        Self {
            resampler: None,
            in_format: Sample::None,
            in_channel_layout: ChannelLayout::STEREO,
            in_rate: 0,
        }
    }

    pub fn resample(&mut self, input: &Audio) -> Result<Audio> {
        let in_format = input.format();
        let in_channel_layout = input.channel_layout();
        let in_rate = input.rate();

        if self.resampler.is_none()
            || self.in_format != in_format
            || self.in_channel_layout != in_channel_layout
            || self.in_rate != in_rate {
            self.resampler = Some(resampling::Context::get(
                in_format,
                in_channel_layout,
                in_rate,
                Sample::F32(Type::Packed),
                ChannelLayout::STEREO,
                in_rate,
            )?);

            self.in_format = in_format;
            self.in_channel_layout = in_channel_layout;
            self.in_rate = in_rate;
        }

        let mut output = Audio::empty();
        let resampler = self.resampler.as_mut().unwrap();
        resampler.run(input, &mut output)?;

        output.set_pts(input.pts());

        Ok(output)
    }
}