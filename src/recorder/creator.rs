use std::ptr::null_mut;
use std::sync::{Arc, Mutex};

use ffmpeg_next::codec::Flags;
use ffmpeg_next::sys::AVPixelFormat;

use crate::recorder::parameters::{BaseParams, VideoParams};
use crate::recorder::recorders::d3d11::{create_id3d11device, get_hw_device_and_frame_cxt};
use crate::recorder::recorders::encoders::{new_video_encoder, VideoEncoderType, VideoFrameType};
use crate::recorder::recorders::frame::create_av_frame;
use crate::recorder::recorders::video::desktop_recorder::VideoCapturer;
use crate::ring_buffer::packet_handlers::KeyFrameStartPacketWrapper;
use crate::ring_buffer::ring_buffer::RingBuffer;

pub fn create_recorder() {
    let video_params = VideoParams {
        base_params: BaseParams {
            bit_rate: 8_000_000,
            max_bit_rate: 10_000_000,
            flags: Flags::GLOBAL_HEADER,
            rate: 30, //fps
        },
        out_width: 1500,
        out_height: 1000,
    };

    let ring_buffer = RingBuffer::<KeyFrameStartPacketWrapper>::new((10 * video_params.base_params.rate) as u32);
    let arc_ring_buffer = Arc::new(Mutex::new(ring_buffer));

    let (device, context, duplication) = create_id3d11device(0).unwrap();
    let (hw_device_ctx, hw_frame_ctx) = get_hw_device_and_frame_cxt(&device, &video_params);
    let video_frame_type = VideoFrameType::D3D11 { hw_device_ctx, hw_frame_ctx };
    let video_encoder_type = VideoEncoderType::HevcAmf;
    let video_encoder = new_video_encoder(&video_params, &video_frame_type, &video_encoder_type).unwrap();
    let av_frame = create_av_frame(AVPixelFormat::AV_PIX_FMT_D3D11, video_params.out_width as i32, video_params.out_height as i32, hw_frame_ctx).unwrap();

    let desktop_recorder = VideoCapturer::new(arc_ring_buffer.clone(), video_encoder, video_params, (device, context, duplication), av_frame);
}