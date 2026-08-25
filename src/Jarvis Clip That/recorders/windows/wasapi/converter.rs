use crate::recorders::traits::Converter;
use crate::recorders::windows::wasapi::env::EnvWasapi;

struct ConverterWasapi {
    in_out_freq: Option<(u32, u32)>,
}

impl ConverterWasapi {
    pub fn new(out_freq: Option<u32>, env: <Self  as Converter>::Env<'_>) -> Self {
        Self {
            in_out_freq: out_freq
                .filter(|&out_freq| out_freq != env.format.Format.nSamplesPerSec)
                .map(|out_freq| (env.format.Format.nSamplesPerSec, out_freq)),
        }
    }
}

impl Converter for ConverterWasapi {
    type Env<'e> = EnvWasapi;
    type Input<'i> = (&'i [u8], u64);
    type Output<'o> = (&'o [u8], u64);

    fn convert<'a>(&'a mut self, input: Self::Input<'a>, env: &Self::Env<'_>) -> anyhow::Result<Self::Output<'a>> {
        Ok(input)
    }
}