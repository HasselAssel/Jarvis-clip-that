use std::thread;
use std::thread::JoinHandle;

use anyhow::Result;
use crossbeam_channel::{Receiver, Sender};
use ffmpeg_next::{codec, Packet, Rational};

use crate::debug;
use crate::recorders::traits::{Converter, Driver, Encoder, RingBuffer, Source};

pub struct RecorderPipeline<Env, D, S, C, E> {
    env: Env,
    driver: D,
    source: S,
    converter: C,
    encoder: E,
}

impl<Env, D, S, C, E> RecorderPipeline<Env, D, S, C, E>
where
    D: Driver<Env>,
    for<'e> S: Source<Env<'e> = Env>,
    for<'e> C: Converter<Env<'e> = Env>,
    for<'e> E: Encoder<Env<'e> = Env>,
    for<'a> S: Source<Output<'a> = <C as Converter>::Input<'a>>,
    for<'a> C: Converter<Output<'a> = <E as Encoder>::Input<'a>>,
{
    pub fn new(
        env: Env,
        driver: D,
        source: S,
        converter: C,
        encoder: E,
    ) -> Self {
        Self {
            env,
            driver,
            source,
            converter,
            encoder,
        }
    }

    fn next(&mut self) -> Result<<E as Encoder>::Output> {
        let source_frame = self.source.next_frame(&self.env)?;
        let converted_frame = self.converter.convert(source_frame, &self.env)?;
        self.encoder.encode(converted_frame, &self.env)
    }

    pub fn start(
        mut self,
        channel_send: Sender<E::Output>,
    ) -> JoinHandle<()>
    where
        D: Send + 'static,
        Env: Send + 'static,
        S: Send + 'static,
        C: Send + 'static,
        E: Send + 'static,
        <E as Encoder>::Output: Send,
    {
        thread::spawn(move || {
            self.driver.start(&mut self.env);
            loop {
                let res = self.next();
                if let Ok(next) = res {
                    if channel_send.send(next).is_err() {
                        return
                    }
                } else {
                    debug!(res.err());
                }

                self.driver.wait(&mut self.env);
            }
        })
    }
}



pub struct RecorderStream<T> {
    pub receiver: Receiver<T>,

    packets_ring_buffer: Box<dyn RingBuffer<Packet> + Send>,

    pub start_time: i64, // in recorder time_base
    pub parameters: codec::Parameters,
    pub time_base: Rational,
}

impl<T> RecorderStream<T> {
    pub fn new<R: RingBuffer<Packet> + Send + 'static> (
        receiver: Receiver<T>,
        retention_secs: i64,
        parameters: codec::Parameters,
        time_base: Rational,
        start_time: i64,
    ) -> Self {
        Self {
            receiver,
            packets_ring_buffer: Box::new(<R as RingBuffer<Packet>>::new(retention_secs * time_base.1 as i64 / time_base.0 as i64)),
            start_time,
            parameters,
            time_base,
        }
    }

    pub fn insert(&mut self, packets: Vec<Packet>) {
        packets.into_iter().for_each(|packet| self.packets_ring_buffer.insert(packet));
    }

    pub fn copy_out(&self) -> Vec<Packet> {
        self.packets_ring_buffer.copy_out()
    }
}