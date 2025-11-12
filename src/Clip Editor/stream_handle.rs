use std::sync::{Arc, mpsc as std_mpsc};
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use ffmpeg_next::media;
use tokio::sync::mpsc as tokio_mpsc;

pub struct StreamScheduler<T> {
    receiver: tokio_mpsc::Receiver<T>,
    //sender: std_mpsc::Sender<T>,
    rate: f64,
    is_playing: Arc<AtomicBool>,

    pub on_send_callback: Arc<dyn Fn(T) + Send + Sync>,
}

impl<T: 'static + Send> StreamScheduler<T> {
    fn start(mut self) {
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(Duration::from_secs_f64(1.0 / self.rate));
            while let Some(t) = self.receiver.recv().await {
                ticker.tick().await;
                (self.on_send_callback)(t);
            }
        });
    }
}

pub struct StreamHandle<T> {
    pub is_playing: Arc<AtomicBool>,
    pub stream_index: usize,
    pub stream_type: media::Type,

    stream_scheduler: Option<StreamScheduler<T>>,
}

impl<T: 'static + Send> StreamHandle<T> {
    pub fn new(stream_index: usize, receiver: tokio_mpsc::Receiver<T>, rate: f64, stream_type: media::Type) -> Self {
        let is_playing = Arc::new(AtomicBool::new(true));
        Self {
            is_playing: is_playing.clone(),
            stream_index,
            stream_type,
            stream_scheduler: Some(StreamScheduler {
                receiver,
                rate,
                is_playing,
                on_send_callback: Arc::new(|_| {}),
            }),
        }
    }

    pub fn set_callback(&mut self, callback: Arc<dyn Fn(T) + Send + Sync>) -> Option<()> {
        self.stream_scheduler.as_mut().map(|mut stream_scheduler| stream_scheduler.on_send_callback = callback)
    }

    pub fn start_scheduler(&mut self) {
        if let Some(scheduler) = self.stream_scheduler.take() {
            scheduler.start();
        }
    }
}