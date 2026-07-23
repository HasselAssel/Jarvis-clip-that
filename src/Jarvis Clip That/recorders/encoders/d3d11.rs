use anyhow::Result;

use crate::recorders::env::EnvD3D11;
use crate::recorders::traits::Encoder;

pub struct EncoderD3D11 {

}

impl EncoderD3D11 {

}

impl Encoder for EncoderD3D11 {
    type Env<'e> = EnvD3D11;
    type Input<'i> = ();
    type Output<'o> = ();

    fn encode(&mut self, input: Self::Input<'_>, env: Self::Env<'_>) -> Result<Self::Output<'_>> {
        todo!()
    }
}