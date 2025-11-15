use std::sync::Arc;
use std::sync::mpsc as sync_mpsc;
use std::thread;

use ffmpeg_next::media::Type;
use rodio::{OutputStream, Sink};
use tokio::sync::mpsc as tokio_mpsc;

use crate::audio_playback::{frame_to_interleaved_f32, LiveSource};
use crate::decoders::DecodedFrame;
use crate::egui::{EditorGui, GUIMessage, WorkerMessage};
use crate::media::Media;
use crate::media_decoder::{AudioSettings, VideoSettings};
use crate::media_playback::MediaPlayback;

pub struct ClipEditor {
    worker_message_sender: sync_mpsc::Sender<WorkerMessage>,
    _worker_message_receiver: Option<sync_mpsc::Receiver<WorkerMessage>>,
    gui_message_receiver: tokio_mpsc::UnboundedReceiver<GUIMessage>,
    _gui_message_sender: Option<tokio_mpsc::UnboundedSender<GUIMessage>>,

}

impl ClipEditor {
    pub fn new() -> Self {
        let (worker_message_sender, worker_message_receiver) = sync_mpsc::channel();
        let (gui_message_sender, gui_message_receiver) = tokio_mpsc::unbounded_channel();

        Self {
            worker_message_sender,
            _worker_message_receiver: Some(worker_message_receiver),
            gui_message_receiver,
            _gui_message_sender: Some(gui_message_sender),
        }
    }

    pub fn start_gui(mut self) {
        let width = 1000;//ToDO
        let height = 800;

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
        let width = 1000;//ToDO
        let height = 800;

        let ctx = ctx_rx.await.unwrap();

        let media = Media::open_file("out/Chat Clip That_20250818_004213.mp4");
        //let media = Media::open_file("out/Marvel-Rivals__2025-03-13__20-58-48.mp4");
        let mut media_playback = MediaPlayback::new(media, VideoSettings { width, height }, AudioSettings);
        {
            let primary_video_handle = media_playback.stream_handles.iter().find(|(i, stream_handle)| stream_handle.stream_type == Type::Video);
            if let Some((i, _)) = primary_video_handle {
                let worker_message_sender = self.worker_message_sender.clone();
                let callback_fn = Arc::new(move |decoded_frame| {
                    if let DecodedFrame::Video(video_frame) = decoded_frame {
                        let worker_message = WorkerMessage::Frame(video_frame);
                        worker_message_sender.send(worker_message).unwrap();
                        ctx.request_repaint()
                    }
                });
                media_playback.add_stream_handle_callback(*i, callback_fn);
            }

            let primary_audio_handle = media_playback.stream_handles.iter().find(|(i, stream_handle)| stream_handle.stream_type == Type::Audio);
            if let Some((i, idk)) = primary_audio_handle {
                if let Some(i_stream) = media_playback.media_decoder.media.streams.get(i) {
                    let (tx, rx) = std::sync::mpsc::channel();

                    let sample_rate = unsafe { (*i_stream.parameters.as_ptr()).sample_rate };
                    let channels = unsafe { (*i_stream.parameters.as_ptr()).ch_layout.nb_channels };

                    thread::spawn(move || {
                        let (_stream, stream_handle) = OutputStream::try_default().unwrap();
                        let sink = Sink::try_new(&stream_handle).unwrap();
                        let source = LiveSource {
                            receiver: rx,
                            sample_rate: sample_rate.try_into().unwrap(),
                            channels: 1//channels.try_into().unwrap(),
                        };
                        sink.append(source);
                        sink.sleep_until_end();
                    });

                    let callback_fn = Arc::new(move |decoded_frame| {
                        if let DecodedFrame::Audio(audio_frame) = decoded_frame {
                            let audio_data = frame_to_interleaved_f32(&audio_frame);
                            audio_data.into_iter().for_each(|d| {let _ = tx.send(d);})
                        }
                    });
                    media_playback.add_stream_handle_callback(*i, callback_fn);
                }
            }
        }
        media_playback.start().await;
    }
}