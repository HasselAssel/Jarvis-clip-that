pub enum AudioCodec {
    AAC,
    Test,
}

pub enum AudioSourceType {
    WasApiSys,
    WasApiProcess { process_id: u32, include_tree: bool },
}