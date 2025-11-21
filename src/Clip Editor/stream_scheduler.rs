use std::fmt::Debug;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use ffmpeg_next::frame;
use tokio::sync::{mpsc as tokio_mpsc, Mutex, Notify};
use tokio::time::Interval;

#[derive(Default)]
pub struct PlayState {
    playing: AtomicBool,
    notify: Notify,
}

impl PlayState {
    pub fn set_playing(&self, v: bool) {
        self.playing.store(v, Ordering::SeqCst);
        self.notify.notify_waiters();
    }

    pub fn flip_playing(&self) {
        self.playing.fetch_xor(true, Ordering::SeqCst);
        self.notify.notify_waiters();
    }

    async fn wait_until_playing(&self) -> bool {
        if self.playing.load(Ordering::SeqCst) {
            return false;
        }
        loop {
            self.notify.notified().await;
            if self.playing.load(Ordering::SeqCst) {
                return true;
            }
        }
    }
}


pub trait StreamFrameScheduler<F> {
    fn insert_frame<'a>(&'a self, frame: F) -> Pin<Box<dyn Future<Output=()> + Send + 'a>>;
    fn start(&mut self);
    fn set_call_back(&mut self, call_back: Arc<AsyncFnType<F>>);
    fn get_play_state(&self) -> Arc<PlayState>;
}


pub type AsyncFnType<F> = dyn Fn(F) -> Pin<Box<dyn Future<Output=()> + Send>> + Send + Sync;

pub struct FixedRateScheduler<F> {
    schedule_sender: tokio_mpsc::Sender<F>,
    play_state: Arc<PlayState>,

    data: Option<(tokio_mpsc::Receiver<F>, Arc<AsyncFnType<F>>, Interval)>,
}

impl<F: HasSamples + Send + 'static> StreamFrameScheduler<F> for FixedRateScheduler<F> {
    fn insert_frame<'a>(&'a self, frame: F) -> Pin<Box<dyn Future<Output=()> + Send + 'a>> {
        Box::pin(async move {
            self.schedule_sender.send(frame).await.unwrap();
        })
    }

    fn start(&mut self) {
        if let Some((mut rx, call_back, mut ticker)) = self.data.take() {
            let play_state = self.play_state.clone();
            tokio::spawn(async move {
                while let Some(f) = rx.recv().await {
                    if play_state.wait_until_playing().await {
                        ticker.reset();
                    }
                    for _ in 0..f.get_samples() {
                        ticker.tick().await;
                    }
                    call_back(f).await;
                }
            });
        }
    }

    fn set_call_back(&mut self, call_back: Arc<AsyncFnType<F>>) {
        if let Some((_, ref mut c, _)) = &mut self.data {
            *c = call_back;
        }
    }

    fn get_play_state(&self) -> Arc<PlayState> {
        self.play_state.clone()
    }
}

impl<F: Send + 'static> FixedRateScheduler<F> {
    pub fn new(rate: f64, max_buffered_seconds: f64, call_back: Arc<AsyncFnType<F>>) -> Self {
        let (tx, mut rx) = tokio_mpsc::channel((rate * max_buffered_seconds) as usize);
        let duration = Duration::from_secs_f64(1.0 / rate);
        let ticker = tokio::time::interval(duration);

        let play_state = Default::default();

        Self {
            schedule_sender: tx,
            play_state,
            data: Some((rx, call_back, ticker)),
        }
    }
}


pub struct DynRateScheduler<F> {
    schedule_sender: tokio_mpsc::UnboundedSender<F>,
    max_buffered_seconds: Duration,
    current_buffered_secs: Arc<Mutex<Duration>>,
    buffer_change: Arc<Notify>,

    play_state: Arc<PlayState>,

    data: Option<(tokio_mpsc::UnboundedReceiver<F>, Arc<AsyncFnType<F>>, Arc<Notify>, Arc<Mutex<Duration>>)>,
}

impl<F: HasDuration + Send + 'static> StreamFrameScheduler<F> for DynRateScheduler<F> {
    fn insert_frame<'a>(&'a self, frame: F) -> Pin<Box<dyn Future<Output=()> + Send + 'a>> {
        Box::pin(async {
            let needed_space = frame.get_duration();
            loop {
                {
                    let mut c_secs = self.current_buffered_secs.lock().await;
                    if *c_secs + needed_space <= self.max_buffered_seconds {
                        *c_secs += needed_space;
                        break;
                    }
                }
                self.buffer_change.notified().await;
            }
            self.schedule_sender.send(frame).unwrap();
        })
    }

    fn start(&mut self) {
        if let Some((mut rx, call_back, buffer_change, current_buffered_secs)) = self.data.take() {
            let play_state = self.play_state.clone();
            tokio::spawn(async move {
                while let Some(f) = rx.recv().await {
                    play_state.wait_until_playing().await;
                    tokio::time::sleep(f.get_duration()).await;
                    let frame_dur = f.get_duration();
                    call_back(f).await;
                    {
                        let mut c_secs = current_buffered_secs.lock().await;
                        *c_secs -= frame_dur;
                    }
                    buffer_change.notify_waiters();
                }
            });
        }
    }

    fn set_call_back(&mut self, call_back: Arc<AsyncFnType<F>>) {
        if let Some((_, ref mut c, _, _)) = &mut self.data {
            *c = call_back;
        }
    }

    fn get_play_state(&self) -> Arc<PlayState> {
        self.play_state.clone()
    }
}

impl<F: HasDuration + Send + 'static> DynRateScheduler<F> {
    pub fn new(max_buffered_seconds: Duration, call_back: Arc<AsyncFnType<F>>) -> Self {
        let current_buffered_secs = Arc::new(Mutex::new(Duration::default()));
        let current_buffered_secs2 = current_buffered_secs.clone();

        let buffer_change = Arc::new(Notify::new());
        let buffer_change2 = Arc::new(Notify::new());

        let play_state = Default::default();

        let (tx, mut rx) = tokio_mpsc::unbounded_channel::<F>();

        Self {
            schedule_sender: tx,
            max_buffered_seconds,
            current_buffered_secs,
            buffer_change,
            play_state,
            data: Some((rx, call_back, buffer_change2, current_buffered_secs2)),
        }
    }
}

trait HasDuration {
    fn get_duration(&self) -> Duration;
}

impl HasDuration for frame::Video {
    fn get_duration(&self) -> Duration {
        //self.pts().unwrap() as usize
        panic!();
        Duration::from_millis(1000) // TodO
    }
}

trait HasSamples {
    fn get_samples(&self) -> usize;
}

impl HasSamples for frame::Video {
    fn get_samples(&self) -> usize {
        1
    }
}

impl HasSamples for frame::Audio {
    fn get_samples(&self) -> usize {
        self.samples()
    }
}