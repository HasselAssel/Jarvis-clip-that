use crate::recorders::traits::Converter;

pub struct DmaBufToAvFrameNv12Converter {

}

impl DmaBufToAvFrameNv12Converter {
    pub fn new() -> Self {
        Self {

        }
    }
}

impl Converter for DmaBufToAvFrameNv12Converter {
    type Env<'e> = EnvXdg;
    type Input<'i> = Option<Arc<DmaBufFrame>>;
    type Output<'o> = Option<AvFrame or smth>;

    fn convert(
        &mut self,
        input: Self::Input<'_>,
        env: &Self::Env<'_>
    ) -> Result<Self::Output<'_>> {

    }
}