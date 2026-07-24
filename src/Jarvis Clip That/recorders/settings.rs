pub struct Settings {
    pub fps: i32,
    pub width: u32,
    pub height: u32,
    pub environment: Environment,
}

pub enum Environment {
    D3D11,
}

pub enum Codec {
    HevcAmf,
}