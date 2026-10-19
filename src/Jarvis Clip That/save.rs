use std::fs::{self, OpenOptions};
use std::path::{Path, PathBuf};

use anyhow::{anyhow, bail, Result};
use ffmpeg::{codec, format, Packet, Rational};
use ffmpeg_next as ffmpeg;
use ffmpeg_next::ffi::{av_compare_ts, av_rescale_q};

use crate::recorders::factory::CommandSave;

/// One track to include in this particular clip. Packets must be in
/// encoder output order, with PTS/DTS expressed in `time_base`.
pub struct SaveStream {
    pub packets: Vec<Packet>,
    pub parameters: codec::Parameters,
    pub time_base: Rational,
    pub start_time: i64, // in time_base
}

pub struct Saver {
    directory: PathBuf,
    next_id: u64,
}

impl Saver {
    pub fn new(directory: impl AsRef<Path>) -> Result<Self> {
        let directory = directory.as_ref().to_path_buf();
        fs::create_dir_all(&directory)?;
        Ok(Self { directory, next_id: 1 })
    }

    pub fn save(&mut self, save: CommandSave) -> Result<PathBuf> {
        if save.streams.is_empty() {
            bail!("no streams provided");
        }

        let path = self.reserve_path()?;
        let result = self.write_clip(&path, save);
        if result.is_err() {
            let _ = fs::remove_file(&path);
        }
        result?;
        Ok(path)
    }

    fn reserve_path(&mut self) -> Result<PathBuf> {
        loop {
            let number = self.next_id;
            self.next_id = self.next_id.checked_add(1)
                .ok_or_else(|| anyhow!("clip ID overflow"))?;
            let path = self.directory.join(format!("clip_{number:06}.mkv"));
            match OpenOptions::new().write(true).create_new(true).open(&path) {
                Ok(_) => return Ok(path),
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(e) => return Err(e.into()),
            }
        }
    }

    fn write_clip(&self, path: &Path, save: CommandSave) -> Result<()> { // Todo: audio still not synced!!! idk why check out folder !!! use time to cut unwanted packets that were still floating in the buffer.
        let mut streams = save.streams;

        for stream in &mut streams {
            let now = unsafe {
                av_rescale_q(
                    save.time_stamp,
                    Rational::from((save.time_base.0 as i32, save.time_base.1 as i32)).into(),
                    stream.time_base.into(),
                )
            };

            let retention = unsafe {
                av_rescale_q(
                    save.retention_secs,
                    Rational(1, 1).into(),
                    stream.time_base.into(),
                )
            };

            let max_pts = now
                .checked_sub(stream.start_time)
                .ok_or_else(|| anyhow!("Timestamp overflow"))?;

            let min_pts = max_pts
                .checked_sub(retention)
                .ok_or_else(|| anyhow!("Timestamp overflow"))?;


            // TODO: possibly removes keyframes
            stream.packets.retain(|packet| {
                packet.pts().is_some_and(|pts| {
                    pts >= min_pts && pts <= max_pts
                })
            });
        }

        let mut start: Option<(i64, Rational)> = None;

        for stream in &streams {
            let Some(min_pts) = stream.packets.iter().filter_map(|p| p.pts()).min() else {
                continue;
            };

            let stream_global_pts = stream.start_time
                .checked_add(min_pts)
                .ok_or_else(|| anyhow!("Timestamp overflow"))?;

            if start.is_none_or(|(start_ts, start_tb)| unsafe {
                av_compare_ts(
                    stream_global_pts,
                    stream.time_base.into(),
                    start_ts,
                    start_tb.into(),
                ) < 0
            }) {
                start = Some((stream_global_pts, stream.time_base));
            }
        }

        let (start_ts, start_tb) =
            start.ok_or_else(|| anyhow!("No packets to save"))?;

        let mut output = format::output_as(path, "matroska")?;
        let mut output_indices = Vec::with_capacity(streams.len());

        for stream in &streams {
            let mut out = output.add_stream(None)?;

            out.set_parameters(stream.parameters.clone());
            out.set_time_base(stream.time_base);

            unsafe {
                (*out.parameters().as_mut_ptr()).codec_tag = 0;
            }

            output_indices.push(out.index());
        }

        output.write_header()?;

        let mut packets = Vec::new();

        for (stream_id, stream) in streams.into_iter().enumerate() {
            let output_index = output_indices[stream_id];

            let output_tb = output
                .stream(output_index)
                .ok_or_else(|| anyhow!("Missing output stream"))?
                .time_base();

            let rescaled_start = unsafe {
                av_rescale_q(
                    start_ts,
                    start_tb.into(),
                    stream.time_base.into(),
                )
            };

            let offset = stream.start_time
                .checked_sub(rescaled_start)
                .ok_or_else(|| anyhow!("Timestamp overflow"))?;

            for mut packet in stream.packets {
                packet.set_pts(
                    packet.pts()
                        .map(|pts| {
                            pts.checked_add(offset)
                                .ok_or_else(|| anyhow!("PTS overflow"))
                        })
                        .transpose()?
                );

                packet.set_dts(
                    packet.dts()
                        .map(|dts| {
                            dts.checked_add(offset)
                                .ok_or_else(|| anyhow!("DTS overflow"))
                        })
                        .transpose()?
                );

                packet.rescale_ts(stream.time_base, output_tb);
                packet.set_stream(output_index);
                packet.set_position(-1);

                packets.push((stream_id, packet, output_tb));
            }
        }

        packets.sort_by(|(a_id, a, a_tb), (b_id, b, b_tb)| {
            let a_ts = a.dts().or(a.pts()).unwrap();
            let b_ts = b.dts().or(b.pts()).unwrap();

            let comparison = unsafe {
                av_compare_ts(
                    a_ts,
                    (*a_tb).into(),
                    b_ts,
                    (*b_tb).into(),
                )
            };

            comparison.cmp(&0).then_with(|| a_id.cmp(b_id))
        });

        for (_, mut packet, _) in packets {
            packet.write_interleaved(&mut output)?;
        }

        output.write_trailer()?;

        Ok(())
    }
}
