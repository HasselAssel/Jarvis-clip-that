use std::collections::VecDeque;

use crate::ring_buffer::traits::{PacketHandler, PacketRingBuffer};
use crate::types::Packet;

pub struct RingBuffer<T: PacketHandler> {
    frame_counter: i64,
    buffer: VecDeque<T>,
    min_frame_amount: i64,
}

impl<T: PacketHandler> PacketRingBuffer for RingBuffer<T> {
    fn insert(
        &mut self,
        packet: Packet,
    ) {
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

    fn copy_out(
        &self,
        min_requested_frames: Option<i64>,
    ) -> Vec<Packet> {
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

    fn new(min_frame_amount: u32) -> Self {
        Self {
            frame_counter: 0,
            buffer: VecDeque::new(),
            min_frame_amount: min_frame_amount as i64,
        }
    }
}