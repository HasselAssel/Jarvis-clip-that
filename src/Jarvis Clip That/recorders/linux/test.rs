
use anyhow::{anyhow, Context, Result};
use ashpd::desktop::screencast::{
    CursorMode, Screencast, SelectSourcesOptions, SourceType,
};
use pipewire as pw;
use pw::{properties::properties, spa};
use spa::pod::Pod;
use std::io::Write;

struct CaptureData {
    format: spa::param::video::VideoInfoRaw,
    saved: bool,
}

pub fn test() -> Result<()> {
    // 1. Authorize capture through XDG Desktop Portal.
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;

    let (portal, session, node_id, fd) = rt.block_on(async {
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

    // 2. Connect to the portal's PipeWire remote.
    pw::init();

    let mainloop = pw::main_loop::MainLoopRc::new(None)?;
    let context = pw::context::ContextRc::new(&mainloop, None)?;
    let core = context.connect_fd(fd, None)?;

    let stream = pw::stream::StreamBox::new(
        &core,
        "screen-test",
        properties! {
            *pw::keys::MEDIA_TYPE => "Video",
            *pw::keys::MEDIA_CATEGORY => "Capture",
            *pw::keys::MEDIA_ROLE => "Screen",
        },
    )?;

    let data = CaptureData {
        format: Default::default(),
        saved: false,
    };

    let loop_for_callback = mainloop.clone();

    // 3. Register format and frame callbacks.
    let _listener = stream
        .add_local_listener_with_user_data(data)
        .state_changed(|_, _, old, new| {
            println!("PipeWire state: {:?} -> {:?}", old, new);
        })
        .param_changed(|_, state, id, param| {
            if id != spa::param::ParamType::Format.as_raw() {
                return;
            }

            if let Some(param) = param {
                if state.format.parse(param).is_ok() {
                    let size = state.format.size();

                    println!(
                        "Negotiated: {}x{} {:?}",
                        size.width,
                        size.height,
                        state.format.format()
                    );
                }
            }
        })
        .process(move |stream, state| {
            if state.saved {
                return;
            }

            let Some(mut buffer) = stream.dequeue_buffer() else {
                return;
            };

            let size = state.format.size();
            let width = size.width;
            let height = size.height;

            if width == 0 || height == 0 {
                return;
            }

            let Some(data) = buffer.datas_mut().first_mut() else {
                return;
            };

            let offset = data.chunk().offset() as usize;
            let chunk_size = data.chunk().size() as usize;
            let stride = data.chunk().stride();

            if stride <= 0 {
                eprintln!("Unsupported stride: {stride}");
                return;
            }

            let stride = stride as usize;
            let row_bytes = width as usize * 3;

            if stride < row_bytes {
                eprintln!("Invalid stride");
                return;
            }

            let Some(bytes) = data.data() else {
                eprintln!("Frame is not CPU-mapped");
                return;
            };

            // Copy visible RGB pixels without row padding.
            let mut rgb = Vec::with_capacity(
                row_bytes * height as usize
            );

            let end = match offset.checked_add(chunk_size) {
                Some(end) if end <= bytes.len() => end,
                _ => return,
            };

            let bytes = &bytes[offset..end];

            for y in 0..height as usize {
                let start = y * stride;
                let Some(row) = bytes.get(start..start + row_bytes)
                else {
                    eprintln!("Frame buffer too small");
                    return;
                };
                rgb.extend_from_slice(row);
            }

            match save_ppm(
    "screenshot.ppm",
    width,
    height,
    stride,
    bytes,
) {
                Ok(()) => println!("Saved screenshot.ppm"),
                Err(e) => eprintln!("Saving failed: {e}"),
            }

            state.saved = true;

            // Stop after the first complete frame.
            loop_for_callback.quit();
        })
        .register()?;

    // 4. Advertise CPU-accessible RGB format.
    let format = spa::pod::object!(
        spa::utils::SpaTypes::ObjectParamFormat,
        spa::param::ParamType::EnumFormat,
        spa::pod::property!(
            spa::param::format::FormatProperties::MediaType,
            Id,
            spa::param::format::MediaType::Video
        ),
        spa::pod::property!(
            spa::param::format::FormatProperties::MediaSubtype,
            Id,
            spa::param::format::MediaSubtype::Raw
        ),
        spa::pod::property!(
            spa::param::format::FormatProperties::VideoFormat,
            Id,
            spa::param::video::VideoFormat::BGRx
        ),
        spa::pod::property!(
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

    let bytes = spa::pod::serialize::PodSerializer::serialize(
        std::io::Cursor::new(Vec::new()),
        &spa::pod::Value::Object(format),
    )?.0.into_inner();

    let mut params = [
        Pod::from_bytes(&bytes)
            .context("Invalid SPA format POD")?
    ];

    println!("Connecting to PipeWire node {node_id}...");

    // 5. Connect to the selected screen.
    stream.connect(
        spa::utils::Direction::Input,
        Some(node_id),
        pw::stream::StreamFlags::AUTOCONNECT
            | pw::stream::StreamFlags::MAP_BUFFERS,
        &mut params,
    )?;

    println!("Stream connected, starting event loop...");

    // Blocks until the callback quits the loop.
    mainloop.run();

    // Keep portal/session alive through capture.
    drop(stream);
    drop(core);
    drop(context);
    drop(session);
    drop(portal);

    Ok(())
}

fn save_ppm(
    path: &str,
    width: u32,
    height: u32,
    stride: usize,
    bgrx: &[u8],
) -> anyhow::Result<()> {
    use std::io::Write;

    let width = width as usize;
    let height = height as usize;
    let row_bytes = width * 4;

    anyhow::ensure!(stride >= row_bytes, "Invalid stride");

    let required = (height - 1) * stride + row_bytes;
    anyhow::ensure!(bgrx.len() >= required, "Buffer too small");

    let mut file = std::fs::File::create(path)?;
    write!(file, "P6\n{width} {height}\n255\n")?;

    for y in 0..height {
        let start = y * stride;
        let row = &bgrx[start..start + row_bytes];

        for pixel in row.chunks_exact(4) {
            file.write_all(&[
                pixel[2], // R
                pixel[1], // G
                pixel[0], // B
            ])?;
        }
    }

    Ok(())
}