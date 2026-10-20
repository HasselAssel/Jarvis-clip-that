use std::collections::VecDeque;

use ffmpeg_next::Packet;

use crate::recorders::traits;
use crate::recorders::traits::PacketHandler;

#[derive(Default)]
pub struct KeyFrameStartPacketWrapper {
    buffer: Vec<Packet>,
}

impl PacketHandler for Packet {
    fn insert(container: &mut VecDeque<Self>, packet: Packet) {
        container.push_back(packet);
    }

    fn get_contents(&self) -> &[Packet] {
        std::slice::from_ref(self)
    }
}

impl PacketHandler for KeyFrameStartPacketWrapper {
    fn insert(container: &mut VecDeque<Self>, packet: Packet) {
        if container.is_empty() || packet.is_key() {
            container.push_back(Self::default());
        }

        container.back_mut().unwrap().buffer.push(packet);

    }

    fn get_contents(&self) -> &[Packet] {
        self.buffer.as_slice()
    }
}

pub struct RingBuffer<P> {
    buffer: VecDeque<P>,
    retention: i64, // in pts time base
    max_pts_duration: Option<i64>,
}

impl<P: PacketHandler> traits::RingBuffer<Packet> for RingBuffer<P> {
    fn new(retention: i64) -> Self {
        Self {
            buffer: VecDeque::new(),
            retention,
            max_pts_duration: None,
        }
    }

    fn insert(&mut self, packet: Packet) {
        if let Some(pts) = packet.pts() {
            let end = pts + packet.duration();
            self.max_pts_duration = Some(
                self.max_pts_duration.unwrap_or(0).max(end)
            );
        }

        P::insert(&mut self.buffer, packet);

        let Some(max_end) = self.max_pts_duration else {
            return;
        };

        let cutoff = max_end - self.retention;

        while let Some(front) = self.buffer.front() {
            let expired = front.get_contents().iter().all(|packet| {
                packet.pts().is_some_and(|pts| {
                    pts.saturating_add(packet.duration()) <= cutoff
                })
            });

            if !expired {
                break;
            }

            self.buffer.pop_front();
        }
    }

    fn copy_out(&self) -> Vec<Packet> {
        self.buffer
            .iter()
            .flat_map(|item| item.get_contents())
            .cloned()
            .collect()
    }
}
