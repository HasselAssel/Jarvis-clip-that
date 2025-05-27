use std::collections::VecDeque;
use std::ops::{Deref, DerefMut};

pub struct PacketWrapper {
    frame_amount: i32,
    buffer: ffmpeg_next::codec::packet::Packet,
}

pub struct PacketWrappersWrapper {
    frame_amount: i32,
    buffer: Vec<PacketWrapper>,
}

pub struct RingBuffer {
    frame_counter: i32,
    buffer: VecDeque<PacketWrappersWrapper>,
    min_frame_amount: i32,
}

impl PacketWrapper {
    pub fn new(frame_amount: i32, buffer: ffmpeg_next::codec::packet::Packet) -> Self {
        Self {
            frame_amount,
            buffer,
        }
    }
}

impl PacketWrappersWrapper {
    pub fn new() -> Self {
        Self {
            frame_amount: 0,
            buffer: Vec::new(),
        }
    }

    pub fn insert(&mut self, packet: PacketWrapper) {
        self.frame_amount += packet.frame_amount;
        self.buffer.push(packet);
    }
}

impl RingBuffer {
    pub fn new(requested_frame_amount: i32) -> Self {
        Self {
            frame_counter: 0,
            buffer: VecDeque::new(),
            min_frame_amount: requested_frame_amount,
        }
    }
    
    pub fn insert(&mut self, packet: PacketWrapper) {
        self.frame_counter += packet.frame_amount;

        while let Some(front) = self.buffer.front_mut() {
            if self.frame_counter - front.frame_amount > self.min_frame_amount {
                self.frame_counter -= front.frame_amount;
                self.buffer.pop_front();
            } else {
                break;
            }
        }

        let packet_wrappers_wrapper = {
            let reuse = self.buffer.back()
                .map(|_back| !packet.is_key())
                .unwrap_or(false);
            if reuse {
                self.buffer.back_mut()
            } else {
                self.buffer.push_back(PacketWrappersWrapper::new());
                self.buffer.back_mut()
            }
        }.unwrap();

        packet_wrappers_wrapper.insert(packet);
    }

    pub fn get_slice(&self, min_requested_frames: Option<i32>) -> Vec<ffmpeg_next::codec::packet::Packet> {
        let returned_vec = if let Some(min_requested_frames) = min_requested_frames {
            let mut i: usize = self.buffer.len();
            let mut frames = 0;
            let mut vec = VecDeque::new();
            while min_requested_frames > frames {
                i -= 1;
                let pww = &self.buffer[i];
                for e in pww.buffer.iter().map(|a| a.buffer.clone()).rev() {
                    vec.push_front(e);
                }// TODO: Needs testing!
                frames += pww.frame_amount;
            }
            vec.into()
        } else {
            let packets: Vec<ffmpeg_next::codec::packet::Packet> = self.buffer.iter().flat_map(|b| b.buffer.iter().map(|b| b.buffer.clone())).collect();
            packets
        };
        returned_vec
    }
}

impl Deref for PacketWrapper {
    type Target = ffmpeg_next::packet::Packet;

    fn deref(&self) -> &Self::Target {
        &self.buffer
    }
}

impl Deref for PacketWrappersWrapper {
    type Target = Vec<PacketWrapper>;

    fn deref(&self) -> &Self::Target {
        &self.buffer
    }
}

impl Deref for RingBuffer {
    type Target = VecDeque<PacketWrappersWrapper>;

    fn deref(&self) -> &Self::Target {
        &self.buffer
    }
}

impl DerefMut for PacketWrapper {
    fn deref_mut(&mut self) -> &mut ffmpeg_next::packet::Packet {
        &mut self.buffer
    }
}

impl DerefMut for PacketWrappersWrapper {
    fn deref_mut(&mut self) -> &mut Vec<PacketWrapper> {
        &mut self.buffer
    }
}

impl DerefMut for RingBuffer {
    fn deref_mut(&mut self) -> &mut VecDeque<PacketWrappersWrapper> {
        &mut self.buffer
    }
}

impl<'a> IntoIterator for &'a mut PacketWrappersWrapper {
    type Item = &'a mut PacketWrapper;
    type IntoIter = std::slice::IterMut<'a, PacketWrapper>;

    fn into_iter(self) -> Self::IntoIter {
        self.buffer.iter_mut()
    }
}

impl<'a> IntoIterator for &'a PacketWrappersWrapper {
    type Item = &'a PacketWrapper;
    type IntoIter = std::slice::Iter<'a, PacketWrapper>;

    fn into_iter(self) -> Self::IntoIter {
        self.buffer.iter()
    }
}

impl Clone for PacketWrapper {
    fn clone(&self) -> Self {
        Self {
            frame_amount: self.frame_amount,
            buffer: self.buffer.clone(),
        }
    }
}