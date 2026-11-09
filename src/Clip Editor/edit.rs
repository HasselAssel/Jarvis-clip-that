pub struct EditConfig {

}

pub struct VideoChunk {
    info: ChunkInfo,
    
}

pub struct AudioChunk {
    info: ChunkInfo,
    volume: f32,
}

pub struct ChunkInfo {
    stream_id: usize,
    chunk_start: u32,
}
