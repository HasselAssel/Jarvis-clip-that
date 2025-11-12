use std::time::Instant;
use ffmpeg_next::ffi::av_hwframe_transfer_data;
use ffmpeg_next::sys::{av_buffer_ref, av_frame_alloc, av_frame_get_buffer, av_frame_get_side_data, av_hwdevice_ctx_create, av_hwframe_ctx_alloc, av_hwframe_ctx_init, AVBufferRef, avcodec_alloc_context3, avcodec_find_decoder_by_name, avcodec_open2, avcodec_parameters_to_context, avcodec_receive_frame, avcodec_send_packet, AVCodecContext, AVCodecParameters, AVHWDeviceType, AVHWFramesContext, AVPacket, AVPixelFormat, SWS_BILINEAR, sws_getContext, sws_scale};

use windows::Win32::Graphics::Direct3D11::*;

pub unsafe fn idk_yet(packet: *mut AVPacket, stream_params: *const AVCodecParameters) -> (Vec<u8>, u32, u32) {
    let idk = std::ffi::CString::new("hevc_d3d11va").unwrap();
    let codec = avcodec_find_decoder_by_name(idk.as_ptr());
    if codec.is_null() {
        panic!("Hardware decoder not found");
    }
    let codec_ctx = avcodec_alloc_context3(codec);
    avcodec_parameters_to_context(codec_ctx, stream_params);

    let mut hw_device_ctx: *mut AVBufferRef = std::ptr::null_mut();
    let ret = av_hwdevice_ctx_create(
        &mut hw_device_ctx,
        AVHWDeviceType::AV_HWDEVICE_TYPE_D3D11VA,
        std::ptr::null(),
        std::ptr::null_mut(),
        0,
    );
    if ret < 0 {
        panic!("Failed to create D3D11 device: {}", ret);
    }
    (*codec_ctx).hw_device_ctx = av_buffer_ref(hw_device_ctx);

    let hw_frames_ref = av_hwframe_ctx_alloc(hw_device_ctx);
    if hw_frames_ref.is_null() {
        panic!("Failed to allocate hwframe context");
    }
    debug_println!("Codecctx width: {}", (*codec_ctx).width);
    debug_println!("Codecctx height: {}", (*codec_ctx).height);
    let frames_ctx = (*hw_frames_ref).data as *mut AVHWFramesContext;
    (*frames_ctx).format = AVPixelFormat::AV_PIX_FMT_D3D11;
    (*frames_ctx).sw_format = AVPixelFormat::AV_PIX_FMT_NV12; // fallback CPU format
    (*frames_ctx).width = (*codec_ctx).width;
    (*frames_ctx).height = (*codec_ctx).height;
    if av_hwframe_ctx_init(hw_frames_ref) < 0 {
        panic!("Failed to init hwframe context");
    }
    (*codec_ctx).hw_frames_ctx = av_buffer_ref(hw_frames_ref);

    if avcodec_open2(codec_ctx, codec, std::ptr::null_mut()) < 0 {
        panic!("Failed to open hardware decoder");
    }




    let frame = av_frame_alloc();
    let instant1 = Instant::now();

    avcodec_send_packet(codec_ctx, packet);
    while avcodec_receive_frame(codec_ctx, frame) == 0 {

        debug_println!("format: {}", (*frame).format);
        debug_println!("data0: {:?}", (*frame).data[0]);
        debug_println!("data1: {:?}", (*frame).data[1]) ;
        let data = (*frame).data[0];
        let texture = &*(data as *mut ID3D11Texture2D);
        let mut desc2 = D3D11_TEXTURE2D_DESC::default();
        unsafe { texture.GetDesc(&mut desc2); }
        debug_println!("GPU texture: {}x{}, format = {:?}", desc2.Width, desc2.Height, desc2.Format);

        // frame is in GPU memory
        // render or process directly
        debug_println!("scaler run: {:?}", instant1.elapsed());
        let cpu_frame = av_frame_alloc();
        av_hwframe_transfer_data(cpu_frame, frame, 0);


        let rgb_frame = av_frame_alloc();
        (*rgb_frame).format = AVPixelFormat::AV_PIX_FMT_RGB24 as i32;
        (*rgb_frame).width  = (*cpu_frame).width;
        (*rgb_frame).height = (*cpu_frame).height;
        av_frame_get_buffer(rgb_frame, 0);

        let sws_ctx = sws_getContext(
            (*cpu_frame).width,
            (*cpu_frame).height,
            unsafe { std::mem::transmute::<i32, AVPixelFormat>((*cpu_frame).format) },
            (*rgb_frame).width,
            (*rgb_frame).height,
            AVPixelFormat::AV_PIX_FMT_RGB24,
            SWS_BILINEAR,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        );

        sws_scale(
            sws_ctx,
            (*cpu_frame).data.as_ptr() as *const *const u8,
            (*cpu_frame).linesize.as_ptr(),
            0,
            (*cpu_frame).height,
            (*rgb_frame).data.as_mut_ptr(),
            (*rgb_frame).linesize.as_ptr(),
        );

        debug_println!("{}", (*rgb_frame).format);

        let width = (*rgb_frame).width as usize;
        let height = (*rgb_frame).height as usize;
        let linesize = (*rgb_frame).linesize[0] as usize;
        let src = (*rgb_frame).data[0];

        let buffer_size = width * height * 3;
        let mut buffer = vec![0; buffer_size];

        for y in 0..height {
            let src_row = src.add(y * linesize);
            let mut dst_row = &mut buffer[y * width * 3 .. (y + 1) * width * 3];
            dst_row.copy_from_slice(std::slice::from_raw_parts(src_row, width * 3));
        }
        return (buffer, width as u32, height as u32);
    }

    return (Vec::new(), 0, 0);
}