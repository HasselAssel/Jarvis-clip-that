use std::collections::HashMap;
use crate::debug_println;

pub struct Media {
    pub streams: HashMap<usize, Stream>,
    pub ictx: ffmpeg_next::format::context::Input,
}

pub struct Stream {
    pub stream_index: usize,
    pub parameters: ffmpeg_next::codec::Parameters,
    pub packet_headers: Vec<PacketIndex>,
}

struct PacketIndex {
    pub pts: Option<i64>,
    pub dts: Option<i64>,
    pub duration: i64,
    pub is_key: bool,
    pub global_packet_index: usize,
}

impl Media {
    pub fn open_file(input_path: &str) -> Self {
        let mut ictx = ffmpeg_next::format::input(input_path).unwrap();

        let mut streams = ictx.streams().into_iter().map(|stream| (stream.index(), Stream { stream_index: stream.index(), parameters: stream.parameters(), packet_headers: Vec::new() })).collect::<HashMap<_, _>>();
        ictx.streams().for_each(|stream| debug_println!("stream {}", stream.index()));

        for (i, (stream, packet)) in ictx.packets().enumerate() {
            let index = stream.index();
            if let Some(stream) = streams.get_mut(&index) {
                debug_println!("stream id {}, dts {:?}, pts {:?}", stream.stream_index, packet.pts().unwrap(), packet.dts().unwrap());
                stream.packet_headers.push(PacketIndex {
                    pts: packet.pts(),
                    dts: packet.dts(),
                    duration: packet.duration(),
                    is_key: packet.is_key(),
                    global_packet_index: i,
                });
            }
        }

        ictx.seek(i64::MIN, std::ops::RangeFull).unwrap();

        Self {
            streams,
            ictx,
        }
    }
}