use std::collections::VecDeque;

use crate::ring_buffer::traits::PacketHandler;
use crate::types::Packet;

#[derive(Default)]
pub struct KeyFrameStartPacketWrapper {
    buffer: Vec<Packet>,
}

impl PacketHandler for Packet {
    fn insert(container: &mut VecDeque<Self>, packet: Packet) {
        container.push_back(packet);
    }

    fn get_duration(&self) -> i64 {
        self.duration()
    }

    fn get_contents(&self) -> &[Packet] {
        std::slice::from_ref(self)
    }
}

impl PacketHandler for KeyFrameStartPacketWrapper {
    fn insert(
        container: &mut VecDeque<Self>,
        packet: Packet,
    ) {
        let needs_new = container.back().map_or(true, |item| { item.buffer.last().expect("Corrupted KeyFrameStartPacketWrapper (no inner element)").is_key() });

        if needs_new {
            container.push_back(KeyFrameStartPacketWrapper::default());
        }

        let target_self = container.back_mut().expect("Corrupted KeyFrameStartPacketWrapper Container (doesn't contain element)");
        target_self.buffer.push(packet);
    }

    fn get_duration(&self) -> i64 {
        self.buffer.iter().map(|item| item.duration()).sum()
    }

    fn get_contents(&self) -> &[Packet] {
        self.buffer.as_slice()
    }
}