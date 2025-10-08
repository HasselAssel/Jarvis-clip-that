use crate::types::Packet;

struct ClipChannel {
    stream: Vec<Packet>,
    rate: i32,
}
struct Clip {
    video_packets: ClipChannel,
    audio_packets: Vec<ClipChannel>,
}