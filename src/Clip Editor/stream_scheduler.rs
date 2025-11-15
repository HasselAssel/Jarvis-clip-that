use std::fmt::Debug;
use std::sync::Arc;
use tokio::sync::{mpsc as tokio_mpsc, Mutex, Notify};
use crossbeam_channel as cbc;

pub trait StreamFrameScheduler<F> {
    async fn insert_frame(&self, frame: F);
}


struct FixedRateScheduler<F> {
    schedule_sender: tokio_mpsc::Sender<F>,
}

impl<F> StreamFrameScheduler<F> for FixedRateScheduler<F> {
    async fn insert_frame(&self, frame: F) {
        self.schedule_sender.send(frame).await.unwrap()
    }
}

impl<F> FixedRateScheduler<F> {
    fn init(rate: usize, buffered_seconds: usize, sender: cbc::Sender<F>) -> Self {
        let (tx, mut rx) = tokio_mpsc::channel(rate * buffered_seconds);
        tokio::spawn(async {
            let mut ticker = tokio::time::interval(Duration::from_secs_f64(1.0 / rate as f64));
            while let Some(t) = rx.recv() {
                ticker.tick().await;
                sender.send(t).unwrap();
            }
        });
        Self {
            schedule_sender: tx,
        }
    }
}


struct DynRateScheduler<F: Duration> {
    schedule_sender: tokio_mpsc::UnboundedSender<F>,
    max_buffered_seconds: usize,
    current_buffered_secs: Arc<Mutex<usize>>,
    buffer_change: Arc<Notify>,
}

impl<F: Duration> StreamFrameScheduler<F> for DynRateScheduler<F> {
    async fn insert_frame(&self, frame: F) {
        let needed_space = frame.get_duration_millis();
        loop {
            {
                let mut c_secs = self.current_buffered_secs.lock().await;
                if *c_secs + needed_space <= self.max_buffered_seconds {
                    *c_secs += needed_space;
                    break
                }
            }
            self.buffer_change.notified().await;
        }
        self.schedule_sender.send(frame).unwrap();
    }
}

impl<F> DynRateScheduler<F> {
    pub fn init(max_buffered_seconds: usize, sender: cbc::Sender<F>) -> Self {
        let current_buffered_secs = Arc::new(Mutex::new(0));
        let current_buffered_secs2 = current_buffered_secs.clone();

        let buffer_change = Arc::new(Notify::new());
        let buffer_change2 = Arc::new(Notify::new());

        let (tx, mut rx) = tokio_mpsc::unbounded_channel();
        tokio::spawn(async {
            while let Some(f) = rx.recv() {
                tokio::time::sleep(f.get_duration()).await;
                let frame_dur = f.get_dur();
                sender.send(f).unwrap();
                {
                    let c_secs = current_buffered_secs2.lock().await;
                    *c_secs -= frame_dur;
                }
                buffer_change2.notify_waiters();
            }
        });

        Self {
            schedule_sender: tx,
            max_buffered_seconds,
            current_buffered_secs,
            buffer_change
        }
    }
}


trait Duration {
    fn get_duration_millis(&self) -> usize;
}