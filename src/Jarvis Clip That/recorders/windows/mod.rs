pub(crate) mod d3d11;
pub(crate) mod wasapi;

pub enum VideoEnvironment {
    D3D11,
}

pub enum AudioEnvironment {
    WasAPI,
}