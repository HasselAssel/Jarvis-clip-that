#[derive(Clone)]
pub struct BaseParams {
    //pub codec: ffmpeg_next::codec::codec::Codec,
    pub bit_rate: usize,
    pub max_bit_rate: usize,
    pub flags: ffmpeg_next::codec::flag::Flags,
    pub rate: i32, // "fps"
}

#[derive(Clone)]
pub struct VideoParams {
    pub base_params: BaseParams,

    pub out_width: u32,
    pub out_height: u32,
    //pub format: ffmpeg_next::util::format::pixel::Pixel,
}

#[derive(Clone)]
pub struct AudioParams {
    pub base_params: BaseParams,

    pub channel_layout: ffmpeg_next::ChannelLayout,
    pub format: ffmpeg_next::util::format::sample::Sample,

}