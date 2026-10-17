use std::collections::VecDeque;

use anyhow::Result;
use ffmpeg_next::Packet;

pub trait Source {
    type Env<'e>;
    type Output<'o> where Self: 'o;
    fn next_frame(&mut self, env: &Self::Env<'_>) -> Result<Self::Output<'_>>;
}

pub trait Converter {
    type Env<'e>;
    type Input<'i> where Self: 'i;
    type Output<'o> where Self: 'o;
    fn convert<'a>(&'a mut self, input: Self::Input<'a>, env: &Self::Env<'_>) -> Result<Self::Output<'a>>;
}

pub trait Encoder {
    type Env<'e>;
    type Input<'i>;
    type Output;
    fn encode(&mut self, input: Self::Input<'_>, env: &Self::Env<'_>) -> Result<Self::Output>;
}

pub trait Event {
    fn start(&mut self);
    fn await_event(&mut self);
}

pub trait Driver<T> {
    fn start(&mut self, source: &mut T);
    fn wait(&mut self, source: &mut T);
}

pub trait PacketHandler {
    fn insert(container: &mut VecDeque<Self>, packet: Packet) where Self: Sized;
    fn get_duration(&self) -> i64;
    fn get_contents(&self) -> &[Packet];
}

pub trait RingBuffer<T> {
    fn new(retention: i64) -> Self where Self: Sized;
    fn insert(&mut self, data: T);
    fn copy_out(&self) -> Vec<T>;
}
