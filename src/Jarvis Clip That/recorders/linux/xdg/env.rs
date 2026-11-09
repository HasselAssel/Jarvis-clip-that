use anyhow::Result;
use anyhow::anyhow;
use std::{
    os::fd::OwnedFd,
    sync::mpsc::Sender,
};
use crate::debug;
use pipewire::spa::param::video::VideoFormat;

use std::collections::HashMap;
use pipewire::spa;
use spa::pod::{self, Pod, PropertyFlags, Value};
use std::os::fd::AsRawFd;
use std::{
    sync::{Arc, Mutex},
    thread,
    rc::Rc,
    cell::RefCell,
    os::fd::BorrowedFd,
};

use pipewire::properties::properties;

use ashpd::desktop::screencast::{
    Screencast,
    SelectSourcesOptions,
    SourceType,
    CursorMode,
};


pub struct EnvXdg {
    current_frame: Arc<Mutex<Option<Arc<DmaBufFrame>>>>,
}

impl EnvXdg {
    pub fn new() -> Self {
        let current_frame = Arc::new(Mutex::new(None));
        let current_frame_clone = current_frame.clone();

        thread::spawn(move || {
            let (release_tx, release_rx) = pipewire::channel::channel::<u64>();
            let state = Rc::new(RefCell::new(
                CaptureState {
                    video_info: Default::default(),
                    pending: HashMap::new(),
                    next_sequence: 0,
                    current_frame: current_frame_clone,
                    release_tx,
                }
            ));


            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()?;
            
            let (portal, session, node_id, fd) = runtime.block_on(async {
                let portal = Screencast::new().await?;
                let session = portal.create_session(Default::default()).await?;

                portal
                    .select_sources(
                        &session,
                        SelectSourcesOptions::default()
                            .set_sources(Some(SourceType::Monitor.into()))
                            .set_cursor_mode(CursorMode::Embedded)
                            .set_multiple(false),
                    )
                    .await?;

                let response = portal
                    .start(&session, None, Default::default())
                    .await?
                    .response()?;

                let node_id = response
                    .streams()
                    .first()
                    .ok_or_else(|| anyhow!("No monitor selected"))?
                    .pipe_wire_node_id();

                let fd = portal
                    .open_pipe_wire_remote(&session, Default::default())
                    .await?;

                Ok::<_, anyhow::Error>((portal, session, node_id, fd))
            })?;
            
            println!("PipeWire node: {node_id}");

            pipewire::init();

            let mainloop = pipewire::main_loop::MainLoopRc::new(None)?;
            let context = pipewire::context::ContextRc::new(&mainloop, None)?;
            let core = context.connect_fd_rc(fd, None)?;

            let registry = core.get_registry_rc()?;
            let registry_clone = registry.clone();

            let stream = pipewire::stream::StreamRc::new(
                core,
                "screen-test",
                properties! {
                    *pipewire::keys::MEDIA_TYPE => "Video",
                    *pipewire::keys::MEDIA_CATEGORY => "Capture",
                    *pipewire::keys::MEDIA_ROLE => "Screen",
                },
            )?;
            let stream_clone = stream.clone();
            
            let nodes = Rc::new(RefCell::new(Vec::new()));
            let nodes_clone = nodes.clone();

            let connected = Rc::new(RefCell::new(false));
            let connected_clone = connected.clone();

            let _registry_listener = registry
                .add_listener_local()
                .global(move |global| {
                    if global.id != node_id || *connected_clone.borrow() {
                        return;
                    }

                    let node: pipewire::node::Node = registry_clone.bind(global).unwrap();

                    let connected_clone = connected_clone.clone();
                    let stream_clone = stream_clone.clone();

                    let listener = node
                        .add_listener_local()
                        .param(move |_, id, index, _, param| {
                            if id != spa::param::ParamType::EnumFormat || *connected_clone.borrow() {
                                return;
                            }
                            if let Some(param) = param {
                                if let Ok((_, Value::Object(object))) = spa::pod::deserialize::PodDeserializer::deserialize_any_from(param.as_bytes()) {
                                    let has_modifier = object.properties.iter().any(|property| {
                                        property.key == spa::param::format::FormatProperties::VideoModifier.as_raw()
                                    });

                                    if has_modifier {
                                        debug!("Selected DMA-BUF format", index);

                                        let bytes = param.as_bytes().to_vec();

                                        let Some(param) = Pod::from_bytes(&bytes) else {
                                            debug!("Invalid format POD");
                                            return;
                                        };

                                        let mut params = [param];

                                        match stream_clone.connect(
                                            spa::utils::Direction::Input,
                                            Some(node_id),
                                            pipewire::stream::StreamFlags::AUTOCONNECT,
                                            &mut params,
                                        ) {
                                            Ok(()) => {
                                                *connected_clone.borrow_mut() = true;
                                                debug!("Connected using producer's DMA-BUF format");
                                            }
                                            Err(err) => {
                                                debug!("Stream connection failed", err);
                                            }
                                        }
                                    }
                                }
                            }
                        })
                        .register();

                    node.enum_params(
                        0,
                        Some(spa::param::ParamType::EnumFormat),
                        0,
                        u32::MAX,
                    );

                    nodes_clone.borrow_mut().push((node, listener));
                })
                .register();

            let _listener = stream.add_local_listener_with_user_data(state.clone())
                .state_changed(|_, _, old, new| {
                    println!("PipeWire: {old:?} -> {new:?}");
                })
                .param_changed(|stream, state, id, param| {
                    let mut state = state.borrow_mut();
                    if id != spa::param::ParamType::Format.as_raw() {
                        return;
                    }

                    if let Some(param) = param {
                        if let Err(err) = state.video_info.parse(param) {
                            debug!(err);
                            return;
                        }

                        let size = state.video_info.size();

                        debug!(
                            state.video_info.size().width,
                            state.video_info.size().height,
                            state.video_info.format(),
                            state.video_info.modifier()
                        );

                        // Request DMA-BUF memory.
                        let buffers = spa::pod::Object {
                            type_: spa::sys::SPA_TYPE_OBJECT_ParamBuffers,
                            id: spa::sys::SPA_PARAM_Buffers,
                            properties: vec![
                                spa::pod::Property::new(
                                    spa::sys::SPA_PARAM_BUFFERS_dataType,
                                    spa::pod::Value::Int(
                                        (1_u32 << spa::sys::SPA_DATA_DmaBuf) as i32
                                    ),
                                ),
                            ],
                        };

                        let bytes = match spa::pod::serialize::PodSerializer::serialize(
                            std::io::Cursor::new(Vec::new()),
                            &spa::pod::Value::Object(buffers),
                        ) {
                            Ok((cursor, _)) => cursor.into_inner(),
                            Err(err) => {
                                debug!(err);
                                return;
                            }
                        };

                        let Some(param) = spa::pod::Pod::from_bytes(&bytes) else {
                            debug!("Invalid buffer POD");
                            return;
                        };

                        debug!("Requesting SPA_DATA_DmaBuf");

                        if let Err(err) = stream.update_params(&mut [param]) {
                            debug!("update_params failed: {err:?}");
                        }
                    }
                })
                .process(|stream, state| {
                    let mut state = state.borrow_mut();
                    let raw = unsafe { stream.dequeue_raw_buffer() };

                    if raw.is_null() {
                        return;
                    }

                    let sequence = state.next_sequence;
                    state.next_sequence += 1;

                    let frame = (|| {
                        let buffer = unsafe { &*raw };
                        let spa_buffer = unsafe { &*buffer.buffer };

                        let datas = unsafe {
                            std::slice::from_raw_parts(
                                spa_buffer.datas,
                                spa_buffer.n_datas as usize,
                            )
                        };

                        anyhow::ensure!(!datas.is_empty(), "No data planes");

                        let mut planes = Vec::with_capacity(datas.len());

                        for data in datas {
                            anyhow::ensure!(
                                data.type_ == spa::sys::SPA_DATA_DmaBuf,
                                "Received non-DMA-BUF frame"
                            );

                            anyhow::ensure!(data.fd >= 0, "Invalid DMA-BUF FD");
                            anyhow::ensure!(!data.chunk.is_null(), "Missing chunk");

                            let chunk = unsafe { &*data.chunk };

                            anyhow::ensure!(
                                chunk.stride > 0,
                                "Unsupported plane stride"
                            );

                            let fd = unsafe {
                                BorrowedFd::borrow_raw(data.fd as i32)
                            }.try_clone_to_owned()?;

                            planes.push(DmaBufPlane {
                                fd,
                                offset: chunk.offset,
                                stride: chunk.stride as u32,
                                size: data.maxsize as usize,
                            });
                        }

                        let size = state.video_info.size();

                        let frame = DmaBufFrame {
                            planes,
                            width: size.width,
                            height: size.height,
                            format: state.video_info.format(),
                            modifier: state.video_info.modifier(),
                            sequence,
                            release: state.release_tx.clone(),
                        };

                        debug!(
                            frame.sequence,
                            frame.width,
                            frame.height,
                            frame.format,
                            frame.modifier
                        );

                        for (i, plane) in frame.planes.iter().enumerate() {
                            debug!(
                                plane.fd.as_raw_fd(),
                                plane.offset,
                                plane.stride
                            );
                        }

                        Ok(frame)
                    })();

                    match frame {
                        Ok(frame) => {
                            state.pending.insert(sequence, raw);

                            *state.current_frame.lock().unwrap() = Some(Arc::new(frame));
                        }
                        Err(err) => {
                            eprintln!("Frame extraction failed: {err:#}");

                            unsafe {
                                stream.queue_raw_buffer(raw);
                            }
                        }
                    }
                })
                .register()?;

            let release_stream = stream.clone();
            let release_state = state.clone();
            let _release_listener = release_rx.attach(mainloop.loop_(), move |id| {
                if let Some(raw) = release_state.borrow_mut().pending.remove(&id) {
                    unsafe {
                        release_stream.queue_raw_buffer(raw);
                    }
                }
            });

            /*let mut format = pod::object!(
                spa::utils::SpaTypes::ObjectParamFormat,
                spa::param::ParamType::EnumFormat,

                pod::property!(
                    spa::param::format::FormatProperties::MediaType,
                    Id,
                    spa::param::format::MediaType::Video
                ),
                pod::property!(
                    spa::param::format::FormatProperties::MediaSubtype,
                    Id,
                    spa::param::format::MediaSubtype::Raw
                ),
                pod::property!(
                    spa::param::format::FormatProperties::VideoFormat,
                    Id,
                    spa::param::video::VideoFormat::BGRA
                ),
                pod::property!(
                    spa::param::format::FormatProperties::VideoSize,
                    Choice,
                    Range,
                    Rectangle,
                    spa::utils::Rectangle {
                        width: 1920,
                        height: 1080,
                    },
                    spa::utils::Rectangle {
                        width: 1,
                        height: 1,
                    },
                    spa::utils::Rectangle {
                        width: 16384,
                        height: 16384,
                    }
                )
            );

            let mut modifier = pod::property!(
                spa::param::format::FormatProperties::VideoModifier,
                Long,
                0_i64 // DRM_FORMAT_MOD_LINEAR
            );

            modifier.flags = PropertyFlags::MANDATORY;
            format.properties.push(modifier);

            let format_bytes = pod::serialize::PodSerializer::serialize(
                std::io::Cursor::new(Vec::new()),
                &Value::Object(format),
            )?
            .0
            .into_inner();

            let mut params = [
                Pod::from_bytes(&format_bytes)
                    .ok_or_else(|| anyhow!("Invalid format POD"))?
            ];

            stream.connect(
                spa::utils::Direction::Input,
                Some(node_id),
                pipewire::stream::StreamFlags::AUTOCONNECT,
                &mut params,
            )?;

            debug!("PipeWire stream connected");*/

            mainloop.run();

            debug!("mainloop done");

            Ok::<(), anyhow::Error>(())
        });

        Self {
            current_frame,
        }
    }
}

struct CaptureState {
    video_info: spa::param::video::VideoInfoRaw,

    pending: HashMap<u64, *mut pipewire::sys::pw_buffer>,
    next_sequence: u64,

    current_frame: Arc<Mutex<Option<Arc<DmaBufFrame>>>>,
    release_tx: pipewire::channel::Sender<u64>,
}

pub struct DmaBufPlane {
    pub fd: OwnedFd,
    pub offset: u32,
    pub stride: u32,
    pub size: usize,
}

pub struct DmaBufFrame {
    pub planes: Vec<DmaBufPlane>,

    pub width: u32,
    pub height: u32,
    pub format: VideoFormat,
    pub modifier: u64,

    pub sequence: u64,

    release: pipewire::channel::Sender<u64>,
}

impl Drop for DmaBufFrame {
    fn drop(&mut self) {
        let _ = self.release.send(self.sequence);
    }
}