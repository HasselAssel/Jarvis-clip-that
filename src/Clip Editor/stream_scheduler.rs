use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

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
    fn insert_frame<'a>(&'a mut self, frame: F) -> Pin<Box<dyn Future<Output=()> + Send + 'a>>;
    fn start(&mut self) -> bool;
    fn set_call_back(&mut self, call_back: Arc<AsyncFnType<F>>);
    fn get_play_state(&self) -> Arc<PlayState>;
    fn get_request_new_channel(&self) -> Arc<AtomicBool>;
}


pub type AsyncFnType<F> = dyn Fn(F) -> Pin<Box<dyn Future<Output=()> + Send>> + Send + Sync;

pub struct FixedRateScheduler<F> {
    schedule_sender: tokio_mpsc::Sender<F>,

    play_state: Arc<PlayState>,
    request_new_channel: Arc<AtomicBool>,

    call_back: Arc<AsyncFnType<F>>,
    interval: Interval,

    receiver: Option<tokio_mpsc::Receiver<F>>,
}

impl<F: HasSamples + Send + 'static> StreamFrameScheduler<F> for FixedRateScheduler<F> {
    fn insert_frame<'a>(&'a mut self, frame: F) -> Pin<Box<dyn Future<Output=()> + Send + 'a>> {
        Box::pin(async move {
            self.schedule_sender.send(frame).await.unwrap();
        })
    }

    fn start(&mut self) -> bool {
        if let Some(mut rx) = self.receiver.take() {
            let play_state = self.play_state.clone();
            let call_back = self.call_back.clone();
            let mut ticker = tokio::time::interval(self.interval.period());
            let request_new_channel = self.request_new_channel.clone();

            tokio::spawn(async move {
                while let Some(f) = rx.recv().await {
                    let samples = f.get_samples();

                    call_back(f).await;

                    if request_new_channel.load(Ordering::SeqCst) {
                        while let Ok(_) = rx.try_recv() {}
                        request_new_channel.store(false, Ordering::SeqCst);
                    }

                    if play_state.wait_until_playing().await {
                        ticker.reset();
                    }
                    for _ in 0..samples {
                        ticker.tick().await;
                    }
                }
            });
            return true;
        }
        false
    }

    fn set_call_back(&mut self, call_back: Arc<AsyncFnType<F>>) {
        self.call_back = call_back;
    }

    fn get_play_state(&self) -> Arc<PlayState> {
        self.play_state.clone()
    }

    fn get_request_new_channel(&self) -> Arc<AtomicBool> {
        self.request_new_channel.clone()
    }
}

impl<F: Send + 'static> FixedRateScheduler<F> {
    pub fn new(rate: f64, max_buffered_seconds: f64, call_back: Arc<AsyncFnType<F>>, request_new_channel: Arc<AtomicBool>) -> Self {
        let buffer_size = (rate * max_buffered_seconds) as usize;
        let (tx, rx) = tokio_mpsc::channel(buffer_size);

        let duration = Duration::from_secs_f64(1.0 / rate);
        let ticker = tokio::time::interval(duration);

        let play_state = Default::default();

        Self {
            schedule_sender: tx,
            play_state,
            request_new_channel,
            call_back,
            interval: ticker,
            receiver: Some(rx),
        }
    }
}


pub struct DynRateScheduler<F> {
    schedule_sender: tokio_mpsc::UnboundedSender<F>,
    max_buffered_seconds: Duration,
    current_buffered_secs: Arc<Mutex<Duration>>,
    buffer_change: Arc<Notify>,

    play_state: Arc<PlayState>,
    request_new_channel: Arc<AtomicBool>,

    call_back: Arc<AsyncFnType<F>>,

    data: Option<(tokio_mpsc::UnboundedReceiver<F>, Arc<Notify>, Arc<Mutex<Duration>>)>,
}

impl<F: HasDuration + Send + 'static> StreamFrameScheduler<F> for DynRateScheduler<F> {
    fn insert_frame<'a>(&'a mut self, frame: F) -> Pin<Box<dyn Future<Output=()> + Send + 'a>> {
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

    fn start(&mut self) -> bool {
        if let Some((mut rx, buffer_change, current_buffered_secs)) = self.data.take() {
            let play_state = self.play_state.clone();
            let call_back = self.call_back.clone();

            tokio::spawn(async move {
                while let Some(f) = rx.recv().await {
                    let frame_dur = f.get_duration();

                    call_back(f).await;
                    play_state.wait_until_playing().await;
                    tokio::time::sleep(frame_dur).await;

                    {
                        let mut c_secs = current_buffered_secs.lock().await;
                        *c_secs -= frame_dur;
                    }
                    buffer_change.notify_waiters();
                }
            });
            return true;
        }
        false
    }

    fn set_call_back(&mut self, call_back: Arc<AsyncFnType<F>>) {
        self.call_back = call_back;
    }

    fn get_play_state(&self) -> Arc<PlayState> {
        self.play_state.clone()
    }

    fn get_request_new_channel(&self) -> Arc<AtomicBool> {
        self.request_new_channel.clone()
    }
}

impl<F: HasDuration + Send + 'static> DynRateScheduler<F> {
    pub fn new(max_buffered_seconds: Duration, call_back: Arc<AsyncFnType<F>>, request_new_channel: Arc<AtomicBool>) -> Self {
        let current_buffered_secs = Arc::new(Mutex::new(Duration::new(0, 0)));
        let current_buffered_secs2 = current_buffered_secs.clone();

        let buffer_change = Arc::new(Notify::new());
        let buffer_change2 = Arc::new(Notify::new());

        let play_state = Default::default();

        let (tx, rx) = tokio_mpsc::unbounded_channel::<F>();

        Self {
            schedule_sender: tx,
            max_buffered_seconds,
            current_buffered_secs,
            buffer_change,
            play_state,
            request_new_channel,
            call_back,
            data: Some((rx, buffer_change2, current_buffered_secs2)),
        }
    }
}

pub trait HasDuration {
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