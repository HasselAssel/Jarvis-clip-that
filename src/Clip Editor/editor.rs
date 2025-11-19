use std::sync::Arc;
use std::sync::mpsc as sync_mpsc;
use std::thread;

use ffmpeg_next::media::Type;
use rodio::{OutputStream, Sink};
use tokio::sync::mpsc as tokio_mpsc;

use crate::audio_playback::{frame_to_interleaved_f32, LiveSource};
use crate::egui::{EditorGui, GUIMessage, WorkerMessage};
use crate::media::Media;
use crate::media_playback::{AudioSettings, MediaPlayback, VideoSettings};


pub struct ClipEditor {
    worker_message_sender: sync_mpsc::Sender<WorkerMessage>,
    _worker_message_receiver: Option<sync_mpsc::Receiver<WorkerMessage>>,
    gui_message_receiver: tokio_mpsc::UnboundedReceiver<GUIMessage>,
    _gui_message_sender: Option<tokio_mpsc::UnboundedSender<GUIMessage>>,

    video_settings: VideoSettings,
    audio_settings: AudioSettings,
}

impl ClipEditor {
    pub fn new(video_settings: VideoSettings, audio_settings: AudioSettings) -> Self {
        let (worker_message_sender, worker_message_receiver) = sync_mpsc::channel();
        let (gui_message_sender, gui_message_receiver) = tokio_mpsc::unbounded_channel();

        Self {
            worker_message_sender,
            _worker_message_receiver: Some(worker_message_receiver),
            gui_message_receiver,
            _gui_message_sender: Some(gui_message_sender),
            video_settings,
            audio_settings,
        }
    }

    pub fn start_gui(mut self) {
        let width = self.video_settings.width;
        let height = self.video_settings.height;


        let worker_message_receiver = self._worker_message_receiver.take().unwrap();
        let gui_message_sender = self._gui_message_sender.take().unwrap();

        let (ctx_tx, ctx_rx) = tokio::sync::oneshot::channel();
        thread::spawn(move || {
            let runtime = tokio::runtime::Runtime::new().unwrap();
            runtime.block_on(async {
                self.start_editor(ctx_rx).await;
            });
        });

        let _ = eframe::run_native(
            "Clip Editor",
            eframe::NativeOptions::default(),
            Box::new(|cc| Ok(Box::new(EditorGui::new(cc, ctx_tx, width, height, worker_message_receiver, gui_message_sender)))),
        );
    }

    async fn start_editor(
        self,
        ctx_rx: tokio::sync::oneshot::Receiver<eframe::egui::Context>,
    ) {
        let ctx = ctx_rx.await.unwrap();

        let media = Media::open_file("out/Chat Clip That_20250818_004213.mp4");
        //let media = Media::open_file("out/Marvel-Rivals__2025-03-13__20-58-48.mp4");
        let mut media_playback = MediaPlayback::new(media, self.video_settings, self.audio_settings, 3.0);
        {
            media_playback.dummy_callback_insert(ctx, self.worker_message_sender);
        }

        let video_handles = media_playback.get_handles();

        tokio::spawn(async {

        });

        media_playback.start().await;
    }
}