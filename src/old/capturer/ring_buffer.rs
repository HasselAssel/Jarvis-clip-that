use std::collections::VecDeque;
use ffmpeg_next::Packet;

pub trait PacketRingBuffer: Sync + Send {
    fn insert(&mut self, packet: Packet);
    fn copy_out(&self, min_requested_frames: Option<i64>) -> Vec<Packet>;
}

trait PacketHandler: Sized + Sync + Send{
    fn insert(container: &mut VecDeque<Self>, packet: Packet);
    fn get_duration(&self) -> i64;
    fn get_contents(&self) -> &[Packet];
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

    fn copy_out(&self, min_requested_frames: Option<i64>) -> Vec<Packet> {
        if let Some(min_requested_frames) = min_requested_frames {
            let mut total = 0;
            let mut result = Vec::new();

            for item in self.buffer.iter().rev() {
                total += item.get_duration();
                result.push(item.get_contents());
                if total >= min_requested_frames {
                    break;
                }
            }

            result.reverse(); // So the order is preserved (oldest first)
            result.into_iter().flatten().cloned().collect::<Vec<Packet>>()
        } else {
            self.buffer.iter().flat_map(|item| item.get_contents()).cloned().collect()
        }
    }
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
    fn insert(container: &mut VecDeque<Self>, packet: Packet) {
        let needs_new = container.back().map_or(true, |item| {item.buffer.last().unwrap().is_key()});

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

    fn get_contents(&self) -> &[Packet] {
        self.buffer.as_slice()
    }
}