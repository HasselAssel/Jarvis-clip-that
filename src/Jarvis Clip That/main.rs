/*use std::path::Path;
use std::thread;
use std::time::Duration;

use anyhow::{anyhow, Result};
use anyhow::{bail, Context};
use ffmpeg_next::{
    codec::Parameters,
    format,
    Packet,
    Rational,
};

use crate::recorders::windows::d3d11::converters::BgraToNv12Converter;
use crate::recorders::windows::d3d11::encoders::EncoderD3D11;
use crate::recorders::windows::d3d11::env::EnvD3D11;
use crate::recorders::windows::d3d11::sources::SourceD3d11;
use crate::recorders::traits::{Converter, Encoder, Source};

mod recorders;
#[path = "../macros.rs"]
mod macros;

fn main() -> Result<()> {
    let codec = ffmpeg_next::encoder::find_by_name("hevc_amf").ok_or_else(|| anyhow!("no codec!"))?;
    let ctx = ffmpeg_next::codec::context::Context::new_with_codec(codec);
    let enc = ctx.encoder().video()?;


    let fps = 10;

    let env = EnvD3D11::new()?;
    let mut source = SourceD3d11::new(
        0,
        &env
    )?;
    let mut converter = BgraToNv12Converter::new(
        (2560, 1440),
        (2560, 1440),
        &env
    )?;
    let mut encoder = EncoderD3D11::new(
        (2560, 1440),
        fps,
        enc,
        codec,
        &env
    )?;

    let params = encoder.get_params();

    let mut total_packets = Vec::with_capacity(150);

    for i in 1..100 {
        if let Ok(acquired_frame) = source.next_frame(&env) {
            if let Ok(texture) = converter.convert(acquired_frame, &env){
                let mut packets = encoder.encode(texture, &env)?;
                for packet in &mut packets {
                    packet.set_pts(Some(i));
                }
                total_packets.extend(packets);
            } else {println!("texture");}
        } else {println!("acquired_frame");}
        thread::sleep(Duration::from_millis(100));
    }

    save_packets_to_mp4(
        "out/TEST.mp4",
        total_packets,
        params,
        Rational::new(1, fps),
        fps as u32,
    )?;


    Ok(())
}


/// Saves one encoded video stream to an MP4 file.
///
/// Requirements:
/// - `packets` must already contain encoded video packets, such as H.264 or HEVC.
/// - Packets must be in decoding/DTS order.
/// - Packet timestamps must use `packet_time_base`.
/// - `parameters` must come from the encoder that produced the packets.
pub fn save_packets_to_mp4<P: AsRef<Path>>(
    file_name: P,
    mut packets: Vec<Packet>,
    parameters: Parameters,
    packet_time_base: Rational,
    fps: u32,
) -> Result<()> {
    if packets.is_empty() {
        bail!("cannot save MP4: packet vector is empty");
    }

    if fps == 0 {
        bail!("cannot save MP4: fps must be greater than zero");
    }

    let file_name = file_name
        .as_ref()
        .to_str()
        .context("output filename is not valid UTF-8")?;

    // Ensure FFmpeg has been initialized. Repeated initialization is fine.
    ffmpeg_next::init().context("failed to initialize FFmpeg")?;

    let mut output =
        format::output_as(file_name, "mp4").context("failed to create MP4 output context")?;

    // Create the single video stream.
    {
        let mut stream = output
            .add_stream(parameters.id())
            .context("failed to add video stream")?;

        stream.set_parameters(parameters);
        stream.set_time_base(packet_time_base);
        stream.set_rate(Rational::new(fps as i32, 1));

        // Avoid carrying an incompatible codec tag into the MP4 container.
        unsafe {
            (*stream.parameters().as_mut_ptr()).codec_tag = 0;
        }
    }

    /*
     * Shift the whole timeline using one common offset.
     *
     * Using the same offset for PTS and DTS preserves the PTS-DTS difference,
     * which matters when the encoder uses B-frames.
     */
    let earliest_timestamp = packets
        .iter()
        .flat_map(|packet| [packet.pts(), packet.dts()])
        .flatten()
        .min();

    if let Some(offset) = earliest_timestamp {
        for packet in &mut packets {
            packet.set_pts(packet.pts().map(|pts| pts - offset));
            packet.set_dts(packet.dts().map(|dts| dts - offset));
        }
    }

    output
        .write_header()
        .context("failed to write MP4 header")?;

    // The MP4 muxer may change the stream time base while writing the header.
    let output_time_base = output
        .stream(0)
        .context("output video stream disappeared")?
        .time_base();

    for mut packet in packets {
        if packet.pts().is_none() && packet.dts().is_none() {
            bail!("encoded packet has neither PTS nor DTS");
        }

        packet.set_stream(0);
        packet.set_position(-1);
        packet.rescale_ts(packet_time_base, output_time_base);

        packet
            .write_interleaved(&mut output)
            .context("failed to write encoded video packet")?;
    }

    output
        .write_trailer()
        .context("failed to finalize MP4 file")?;

    Ok(())
}*/

mod recorders;

pub fn main() {

}