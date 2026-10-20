use anyhow::Result;

use crate::recorders::traits::Converter;
use crate::recorders::windows::wasapi::env::EnvWasapi;
use crate::recorders::windows::wasapi::sources::WasApiAudioData;

pub struct ConverterWasapi {
    in_out_freq: Option<(u32, u32)>,
}

impl ConverterWasapi {
    pub fn new(out_freq: Option<u32>, env: &<Self  as Converter>::Env<'_>) -> Self {
        Self {
            in_out_freq: out_freq
                .filter(|&out_freq| out_freq != env.format.Format.nSamplesPerSec)
                .map(|out_freq| (env.format.Format.nSamplesPerSec, out_freq)),
        }
    }
}

impl Converter for ConverterWasapi {
    type Env<'e> = EnvWasapi;
    type Input<'i> = WasApiAudioData<'i>;
    type Output<'o> = WasApiAudioData<'o>;

    fn convert<'a>(&'a mut self, input: Self::Input<'a>, _: &Self::Env<'_>) -> Result<Self::Output<'a>> {
        // todo: change frequency if wanted.
        Ok(input)
    }
}