pub enum AudioCodec {
    AAC,
}

pub enum AudioSourceType {
    WasApiDefaultSys,
    WasApiProcess { process_id: u32, include_tree: bool },
    WasApiDefaultInput,
}