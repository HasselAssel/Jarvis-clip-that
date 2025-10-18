pub enum AudioCodec {
    AAC,
    Test,
}

pub enum AudioSourceType {
    WasApiDefaultSys,
    WasApiProcess { process_id: u32, include_tree: bool },
    WasApiDefaultInput,
}