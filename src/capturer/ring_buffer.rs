use std::collections::VecDeque;
use ffmpeg_next::Packet;

pub trait PacketRingBuffer: Sync + Send {
    fn insert(&mut self, packet: Packet);
    fn copy_out(&self, min_requested_frames: Option<i32>) -> Vec<Packet>;
}

trait PacketHandler: Sized + Sync + Send{
    fn insert(container: &mut VecDeque<Self>, packet: Packet);
    fn get_duration(&self) -> i64;
}

pub struct RingBuffer<T: PacketHandler> {
    frame_counter: i64,
    buffer: VecDeque<T>,
    min_frame_amount: i64,
}

#[derive(Default)]
pub struct KeyFrameStartPacketWrapper {
    buffer: Vec<Packet>,
}

impl<T: PacketHandler> RingBuffer<T> {
    pub fn new(min_frame_amount: u32) -> Self {
        Self {
            frame_counter: 0,
            buffer: VecDeque::new(),
            min_frame_amount: min_frame_amount as i64,
        }
    }
}

impl<T: PacketHandler> PacketRingBuffer for RingBuffer<T> {
    fn insert(&mut self, packet: Packet) {
        self.frame_counter += packet.duration();

        T::insert(&mut self.buffer, packet);

        while let Some(front) = self.buffer.front() {
            if self.frame_counter - front.get_duration() > self.min_frame_amount {
                self.frame_counter -= front.get_duration();
                self.buffer.pop_front();
            } else {
                break;
            }
        }
    }

    fn copy_out(&self, min_requested_frames: Option<i32>) -> Vec<Packet> {
        todo!()
    }
}

impl PacketHandler for Packet {
    fn insert(container: &mut VecDeque<Self>, packet: Packet) {
        container.push_back(packet);
    }

    fn get_duration(&self) -> i64 {
        self.duration()
    }
}

impl PacketHandler for KeyFrameStartPacketWrapper {
    fn insert(container: &mut VecDeque<Self>, packet: Packet) {
        let needs_new = container.back().map_or(true, |item| item.buffer.last().unwrap().is_key());

        if needs_new {
            container.push_back(KeyFrameStartPacketWrapper::default());
        }

        // Now it's guaranteed to be non-empty and valid
        let target_self = container.back_mut().unwrap();
        target_self.buffer.push(packet);
    }

    fn get_duration(&self) -> i64 {
        self.buffer.iter().map(|item| item.duration()).sum()
    }
}