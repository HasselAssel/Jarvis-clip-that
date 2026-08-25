use anyhow::Result;

use crate::recorders::traits::{Converter, Encoder, Source};

pub struct Recorder<Env, S, C, E> {
    env: Env,
    source: S,
    converter: C,
    encoder: E,
}

impl<Env, S, C, E> Recorder<Env, S, C, E>
where
    for<'e> S: Source<Env<'e> = Env>,
    for<'e> C: Converter<Env<'e> = Env>,
    for<'e> E: Encoder<Env<'e> = Env>,
    for<'a> S: Source<Output<'a> = <C as Converter>::Input<'a>>,
    for<'a> C: Converter<Output<'a> = <E as Encoder>::Input<'a>>,
{
    pub fn new(
        env: Env,
        source: S,
        converter: C,
        encoder: E,
    ) -> Self {
        Self {
            env,
            source,
            converter,
            encoder,
        }
    }

    pub fn next(&mut self) -> Result<<E as Encoder>::Output<'_>>{
        let source_frame = self.source.next_frame(&self.env)?;
        let converted_frame = self.converter.convert(source_frame, &self.env)?;
        self.encoder.encode(converted_frame, &self.env)
    }
}