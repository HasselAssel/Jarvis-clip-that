use anyhow::Result;

pub trait Source {
    type Env<'e>;
    type Output<'o> where Self: 'o;
    fn next_frame(&mut self, env: Self::Env<'_>) -> Result<Self::Output<'_>>;
}

pub trait Converter {
    type Env<'e>;
    type Input<'i>;
    type Output<'o> where Self: 'o;
    fn convert(&mut self, input: Self::Input<'_>, env: Self::Env<'_>) -> Result<Self::Output<'_>>;
}

pub trait Encoder {
    type Env<'e>;
    type Input<'i>;
    type Output<'o> where Self: 'o;
    fn encode(&mut self, input: Self::Input<'_>, env: Self::Env<'_>) -> Result<Self::Output<'_>>;
}
