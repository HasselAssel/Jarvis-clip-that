use std::ptr::null_mut;

use anyhow::{anyhow, bail, Result};
use ffmpeg_next::encoder::video;
use ffmpeg_next::ffi::av_buffer_create;
use ffmpeg_next::ffi::AVFrame;
use windows::Win32::Graphics::Direct3D11::ID3D11Texture2D;
use windows_core::Interface;

use crate::recorders::env::EnvD3D11;
use crate::recorders::traits::Encoder;

pub struct EncoderD3D11 {
    encoder: video::Video
}

impl EncoderD3D11 {
    pub fn new(env: <Self as Encoder>::Env<'_>) {

    }
}

impl Encoder for EncoderD3D11 {
    type Env<'e> = EnvD3D11;
    type Input<'i> = ();
    type Output<'o> = ();

    fn encode(&mut self, input: Self::Input<'_>, env: Self::Env<'_>) -> Result<Self::Output<'_>> {
        todo!()
    }
}

impl EncoderD3D11 {
    fn prepare_frame(
        &self,
        av_frame: &mut AVFrame,
        texture: &ID3D11Texture2D,
    ) -> Result<()> {
        let texture_buffer = unsafe {
            av_buffer_create(
                texture.as_raw() as _,
                size_of::<*mut ID3D11Texture2D>(),
                None, //Some(Self::buffer_free),
                null_mut(),
                0,
            )
        };
        if texture_buffer.is_null() {
            bail!("cant create buffer");
        }
        unsafe {
            (*av_frame).data[0] = texture.as_raw() as _;
            (*av_frame).buf[0] = texture_buffer;
        }

        Ok(())
    }
}