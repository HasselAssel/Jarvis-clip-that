use std::thread;
use std::time::{Duration, Instant};

use crate::recorders::traits::{Driver, Event};

pub struct PollingDriver {
    interval: Duration,
    next: Instant,
}

impl PollingDriver {
    pub fn new(interval: Duration) -> Self {
        Self {
            interval,
            next: Instant::now(),
        }
    }
}

impl<T> Driver<T> for PollingDriver {
    fn start(&mut self, _: &mut T) {
        self.next = Instant::now();
    }

    fn wait(&mut self, _source: &mut T) {
        self.next += self.interval;

        if let Some(remaining) = self.next.checked_duration_since(Instant::now()) {
            thread::sleep(remaining);
        }
    }
}

pub struct EventDriver;

impl<T: Event> Driver<T> for EventDriver {
    fn start(&mut self, source: &mut T) {
        source.start();
    }

    fn wait(&mut self, source: &mut T) {
        source.await_event();
    }
}