use std::collections::VecDeque;

use crate::types::Packet;

pub trait PacketRingBuffer: Sync + Send {
    fn insert(&mut self, packet: Packet);
    fn copy_out(&self, min_requested_frames: Option<i64>) -> Vec<Packet>;
}

pub trait PacketHandler: Sized + Sync + Send{
    fn insert(container: &mut VecDeque<Self>, packet: Packet);
    fn get_duration(&self) -> i64;
    fn get_contents(&self) -> &[Packet];
}