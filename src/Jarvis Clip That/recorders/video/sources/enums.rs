pub enum VideoCodec {
    Amf, // AMD
    Qsv, // Intel
}

pub enum VideoSourceType {
    D3d11 {monitor_id: u32},
}