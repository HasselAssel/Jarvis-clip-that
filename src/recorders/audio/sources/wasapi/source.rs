use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use ffmpeg_next::encoder::audio::Encoder;
use ffmpeg_next::frame::Audio;
use windows::Win32::Media::Audio::{AUDCLNT_SHAREMODE_SHARED, AUDCLNT_STREAMFLAGS_EVENTCALLBACK, AUDCLNT_STREAMFLAGS_LOOPBACK, eConsole, eRender, IAudioCaptureClient, IAudioClient, IMMDeviceEnumerator, MMDeviceEnumerator, WAVEFORMATEX};
use windows::Win32::System::Com::CoCreateInstance;
use windows::Win32::System::Performance::{QueryPerformanceCounter, QueryPerformanceFrequency};
use windows::Win32::System::Threading::{CreateEventW, INFINITE, WaitForSingleObject};

use crate::recorders::audio::sources::traits::AudioSource;
use crate::recorders::audio::sources::wasapi::traits::WasapiEncoderCtx;
use crate::ring_buffer::traits::PacketRingBuffer;
use crate::wrappers::{MaybeSafeComWrapper, MaybeSafeHANDLEWrapper};
use crate::types::Result;

pub struct AudioSourceWasapi<E: WasapiEncoderCtx> {
    client: MaybeSafeComWrapper<IAudioClient>,
    pub format: WAVEFORMATEX,

    capture_client: MaybeSafeComWrapper<IAudioCaptureClient>,

    event: MaybeSafeHANDLEWrapper,

    frequency: i64,
    start_time: i64,
    pts_counter: i64,
    audio_buffer: VecDeque<u8>,

    context_encoder: E,
}

impl<E: WasapiEncoderCtx> AudioSourceWasapi<E> {
    pub fn new(context_encoder: E) -> Self {
        let (client, format) = create_default_iaudioclient().unwrap();
        let client = MaybeSafeComWrapper(client);

        let event;
        unsafe {
            event = CreateEventW(None, false, false, None).unwrap();
            client.SetEventHandle(event).unwrap();
        }
        let event = MaybeSafeHANDLEWrapper(event);

        let capture_client = unsafe { client.GetService().unwrap() };
        let capture_client = MaybeSafeComWrapper(capture_client);

        Self {
            client,
            format,

            capture_client,

            event,

            frequency: 0,
            start_time: 0,
            pts_counter: 0,
            audio_buffer: VecDeque::new(),

            context_encoder,
        }
    }
}

impl<E: WasapiEncoderCtx> AudioSource for AudioSourceWasapi<E> {
    fn init(&mut self) {
        unsafe {
            QueryPerformanceFrequency(&mut self.frequency).unwrap();
            QueryPerformanceCounter(&mut self.start_time).unwrap();
        }
        unsafe { self.client.Start().unwrap(); }
    }

    fn await_new_audio(&mut self) {
        unsafe { WaitForSingleObject(*self.event, INFINITE); }
    }

    fn gather_new_audio<PRB: PacketRingBuffer>(&mut self, ring_buffer: &Arc<Mutex<PRB>>, encoder: &mut Encoder, frame: &mut Audio, silent_frame: &mut Audio) -> Result<()> {
        let mut packet_length = 0;
        let mut data = std::ptr::null_mut();
        let mut flags = 0;

        let mut device_pos = 0;
        let mut qpc_pos = 0;
        unsafe {
            self.capture_client.GetBuffer(
                &mut data,
                &mut packet_length,
                &mut flags,
                Some(&mut device_pos),
                Some(&mut qpc_pos),
            )?;
        }

        self.context_encoder.process_audio(ring_buffer, encoder, frame, silent_frame, packet_length, data, qpc_pos, self.start_time, self.frequency, &self.format, &mut self.pts_counter, &mut self.audio_buffer, &self.capture_client);

        Ok(())
    }
}


pub fn create_default_iaudioclient() -> Result<(IAudioClient, WAVEFORMATEX)> {
    let try_init = unsafe {
        windows::Win32::System::Com::CoInitializeEx(None, windows::Win32::System::Com::COINIT_MULTITHREADED)
    };
    if try_init.is_err() {
        println!("COM already co-initialized: {}", try_init)
    }

    let enumerator: IMMDeviceEnumerator = unsafe {
        CoCreateInstance(
            &MMDeviceEnumerator,
            None,
            windows::Win32::System::Com::CLSCTX_ALL,
        ).unwrap()
    };

    let device = unsafe {
        enumerator.GetDefaultAudioEndpoint(
            eRender,
            eConsole,
        ).unwrap()
    };

    let client: IAudioClient = unsafe {
        device.Activate(
            windows::Win32::System::Com::CLSCTX_ALL,
            None,
        ).unwrap()
    };

    let format = unsafe { client.GetMixFormat().unwrap() };

    unsafe {
        client.Initialize(
            AUDCLNT_SHAREMODE_SHARED,
            AUDCLNT_STREAMFLAGS_LOOPBACK | AUDCLNT_STREAMFLAGS_EVENTCALLBACK,
            10_000_000,
            0,
            format,
            None,
        ).unwrap();
    }
    let format = unsafe { *format };

    Ok((client, format))
}