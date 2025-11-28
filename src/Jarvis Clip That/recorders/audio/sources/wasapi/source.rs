use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};
use ffmpeg_next::encoder::audio::Encoder;
use ffmpeg_next::frame::Audio;
use windows::Win32::Media::Audio::{AUDCLNT_SHAREMODE_SHARED, AUDCLNT_STREAMFLAGS_EVENTCALLBACK, AUDCLNT_STREAMFLAGS_LOOPBACK, PROCESS_LOOPBACK_MODE_EXCLUDE_TARGET_PROCESS_TREE, PROCESS_LOOPBACK_MODE_INCLUDE_TARGET_PROCESS_TREE, AUDIOCLIENT_PROCESS_LOOPBACK_PARAMS, AUDIOCLIENT_ACTIVATION_PARAMS_0, AUDIOCLIENT_ACTIVATION_TYPE_PROCESS_LOOPBACK, AUDIOCLIENT_ACTIVATION_PARAMS, eConsole, eRender, IAudioCaptureClient, IAudioClient, IMMDeviceEnumerator, MMDeviceEnumerator, WAVEFORMATEX, IAudioSessionManager2, eMultimedia, IAudioSessionControl2, WAVEFORMATEXTENSIBLE, WAVEFORMATEXTENSIBLE_0, AudioSessionState, AudioSessionDisconnectReason, AudioSessionStateExpired, eCapture};
use windows::Win32::System::Com::{BLOB, CLSCTX_ALL, CoCreateInstance};
use windows::Win32::System::Performance::{QueryPerformanceCounter, QueryPerformanceFrequency};
use windows::Win32::System::Threading::{CreateEventW, INFINITE, WaitForSingleObject};
use windows::Win32::System::Com::StructuredStorage::{PROPVARIANT, PROPVARIANT_0, PROPVARIANT_0_0, PROPVARIANT_0_0_0};

use std::{
    mem::ManuallyDrop,
    pin::Pin,
    sync::Condvar,
};
use std::ops::Deref;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use windows::Win32::Media::Audio as WinAudio;
use windows::Win32::System::Variant::VT_BLOB;
use windows::core::{Interface, HRESULT, IUnknown};
use windows_core::{BOOL, GUID, PCWSTR};
use crate::debug_println;
use crate::error::{CustomError, Error};
use crate::recorders::audio::sources::enums::{AudioCodec, AudioSourceType};

use crate::recorders::audio::sources::traits::AudioSource;
use crate::recorders::audio::sources::wasapi::traits::WasapiEncoderCtx;
use crate::recorders::recorder::{create_audio_recorder, Recorder};
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
    fn new(
        context_encoder: E,
        client: IAudioClient,
        format: WAVEFORMATEX,
    ) -> Result<Self> {
        let client = MaybeSafeComWrapper(client);

        let event;
        unsafe {
            event = CreateEventW(None, false, false, None)?;
            client.SetEventHandle(event)?;
        }
        let event = MaybeSafeHANDLEWrapper(event);

        let capture_client = unsafe { client.GetService()? };
        let capture_client = MaybeSafeComWrapper(capture_client);

        Ok(Self {
            client,
            format,

            capture_client,

            event,

            frequency: 0,
            start_time: 0,
            pts_counter: 0,
            audio_buffer: VecDeque::new(),

            context_encoder,
        })
    }

    pub fn new_default(
        context_encoder: E,
        render_else_capture: bool,
    ) -> Result<Self> {
        let (client, format) = create_default_iaudioclient(render_else_capture)?;
        Self::new(context_encoder, client, format)
    }

    pub fn new_process(
        context_encoder: E,
        process_id: u32,
        include_tree: bool,
    ) -> Result<Self> {
        let (client, format) = create_process_iaudioclient(process_id, include_tree)?;
        Self::new(context_encoder, client, format)
    }
}

impl<E: WasapiEncoderCtx> AudioSource for AudioSourceWasapi<E> {
    fn init(&mut self) -> Result<()> {
        unsafe {
            QueryPerformanceFrequency(&mut self.frequency)?;
            QueryPerformanceCounter(&mut self.start_time)?;
        }
        unsafe { self.client.Start()? }
        Ok(())
    }

    fn await_new_audio(&mut self) {
        unsafe { WaitForSingleObject(*self.event, INFINITE); }
    }

    fn gather_new_audio<PRB: PacketRingBuffer>(
        &mut self,
        ring_buffer: &Arc<Mutex<PRB>>,
        encoder: &mut Encoder,
        frame: &mut Audio,
        silent_frame: &mut Audio,
    ) -> Result<()> {
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

        self.context_encoder.process_audio(ring_buffer, encoder, frame, silent_frame, packet_length, data, qpc_pos, self.start_time, self.frequency, &self.format, &mut self.pts_counter, &mut self.audio_buffer, &self.capture_client)
    }
}


pub fn create_default_iaudioclient(render_else_capture: bool) -> Result<(IAudioClient, WAVEFORMATEX)> {
    let try_init = unsafe {
        windows::Win32::System::Com::CoInitializeEx(None, windows::Win32::System::Com::COINIT_MULTITHREADED)
    };
    if try_init.is_err() {
        debug_println!("COM already co-initialized: {}", try_init)
    }

    let enumerator: IMMDeviceEnumerator = unsafe {
        CoCreateInstance(
            &MMDeviceEnumerator,
            None,
            CLSCTX_ALL,
        )?
    };

    let dataflow = match render_else_capture {
        true => { eRender }
        false => { eCapture }
    };
    let device = unsafe {
        enumerator.GetDefaultAudioEndpoint(
            dataflow,
            eConsole,
        )?
    };

    let client: IAudioClient = unsafe {
        device.Activate(
            CLSCTX_ALL,
            None,
        )?
    };

    let format = unsafe { client.GetMixFormat()? };
    let streamflags = AUDCLNT_STREAMFLAGS_EVENTCALLBACK | match render_else_capture {
        true => { AUDCLNT_STREAMFLAGS_LOOPBACK }
        false => { 0 }
    };
    unsafe {
        client.Initialize(
            AUDCLNT_SHAREMODE_SHARED,
            streamflags,
            500_000,
            0,
            format,
            None,
        )?;
    }

    let format = unsafe { *format };

    let f1 = format.wFormatTag;
    debug_println!("{}", f1);
    let f1 = format.nChannels;
    debug_println!("{}", f1);
    let f1 = format.nSamplesPerSec;
    debug_println!("{}", f1);
    let f1 = format.nAvgBytesPerSec;
    debug_println!("{}", f1);
    let f1 = format.nBlockAlign;
    debug_println!("{}", f1);
    let f1 = format.wBitsPerSample;
    debug_println!("{}", f1);
    let f1 = format.cbSize;
    debug_println!("{}", f1);

    Ok((client, format))
}

fn create_process_iaudioclient(
    process_id: u32,
    include_tree: bool,
) -> Result<(IAudioClient, WAVEFORMATEX)> {
    let _try_init = unsafe {
        windows::Win32::System::Com::CoInitializeEx(None, windows::Win32::System::Com::COINIT_MULTITHREADED)
    };

    // Build AUDIOCLIENT_ACTIVATION_PARAMS
    let mut act_params = AUDIOCLIENT_ACTIVATION_PARAMS {
        ActivationType: AUDIOCLIENT_ACTIVATION_TYPE_PROCESS_LOOPBACK,
        Anonymous: AUDIOCLIENT_ACTIVATION_PARAMS_0 {
            ProcessLoopbackParams: AUDIOCLIENT_PROCESS_LOOPBACK_PARAMS {
                TargetProcessId: process_id,
                ProcessLoopbackMode: if include_tree {
                    PROCESS_LOOPBACK_MODE_INCLUDE_TARGET_PROCESS_TREE
                } else {
                    PROCESS_LOOPBACK_MODE_EXCLUDE_TARGET_PROCESS_TREE
                },
            },
        },
    };
    let pinned_params = Pin::new(&mut act_params);

    // Wrap into PROPVARIANT as BLOB
    let raw = PROPVARIANT {
        Anonymous: PROPVARIANT_0 {
            Anonymous: ManuallyDrop::new(PROPVARIANT_0_0 {
                vt: VT_BLOB,
                wReserved1: 0,
                wReserved2: 0,
                wReserved3: 0,
                Anonymous: PROPVARIANT_0_0_0 {
                    blob: BLOB {
                        cbSize: size_of::<AUDIOCLIENT_ACTIVATION_PARAMS>() as u32,
                        pBlobData: pinned_params.get_mut() as *mut _ as *mut _,
                    },
                },
            }),
        },
    };


    let activation_prop = ManuallyDrop::new(raw);
    let pinned_prop = Pin::new(activation_prop.deref());
    let activation_params = Some(pinned_prop.get_ref() as *const _);

    // Setup the handler for async activation
    let pair = Arc::new((Mutex::new(false), Condvar::new()));

    //let handler = Handler::new(pair.clone());
    let handler: WinAudio::IActivateAudioInterfaceCompletionHandler = Handler { pair: pair.clone() }.into();

    // Call ActivateAudioInterfaceAsync
    let operation = unsafe {
        WinAudio::ActivateAudioInterfaceAsync(
            WinAudio::VIRTUAL_AUDIO_DEVICE_PROCESS_LOOPBACK,
            &IAudioClient::IID,
            activation_params,
            &handler,
        )?
    };

    // Wait for callback (blocking)
    {
        let (lock, cvar) = &*pair;
        let mut guard = lock.lock().unwrap();
        while !*guard {
            guard = cvar.wait(guard).unwrap();
        }
    }

    // Retrieve result
    let mut result: HRESULT = HRESULT(0);
    let mut unknown: Option<IUnknown> = None;
    unsafe { operation.GetActivateResult(&mut result, &mut unknown)? };
    result.ok()?;  // check success

    // Downcast to IAudioClient
    let client: IAudioClient = unknown.ok_or(CustomError::CUSTOM(Error::Unknown))?.cast()?;

    let format = new_waveformatextensible(32, 32, 44100, 2, None);

    unsafe {
        client.Initialize(
            AUDCLNT_SHAREMODE_SHARED,
            AUDCLNT_STREAMFLAGS_LOOPBACK
                | AUDCLNT_STREAMFLAGS_EVENTCALLBACK,
            500_000,
            0,
            &format.Format,
            None,
        )?;
    }

    Ok((client, format.Format))
}


pub struct AudioProcessWatcher<PRB: PacketRingBuffer> {
    pub audio_recorders: Arc<tokio::sync::Mutex<HashMap<u32, (Recorder<PRB>, String, Arc<AtomicBool>)>>>,
    _audio_process_watcher: Option<_AudioProcessWatcher<PRB>>,
}

impl<PRB: PacketRingBuffer + 'static> AudioProcessWatcher<PRB> {
    pub fn new(
        audio_codec: AudioCodec,
        include_tree: bool,
        min_secs: u32,
        start_delay_secs: f64,
    ) -> Result<Self> {
        let audio_recorders = Arc::new(tokio::sync::Mutex::new(HashMap::new()));
        let a = audio_recorders.clone();
        Ok(Self {
            audio_recorders,
            _audio_process_watcher: Some(_AudioProcessWatcher::new(audio_codec, include_tree, min_secs, a, start_delay_secs, Instant::now())?),
        })
    }

    pub async fn start_recording(&mut self) -> Result<bool> {
        if let Some(recorder) = self._audio_process_watcher.take() {
            return recorder.start_listening().await.and(Ok(true));
        }
        Ok(false)
    }
}

struct _AudioProcessWatcher<PRB: PacketRingBuffer> {
    session_manager: IAudioSessionManager2,

    audio_codec: AudioCodec,
    include_tree: bool,
    min_secs: u32,
    audio_recorders: Arc<tokio::sync::Mutex<HashMap<u32, (Recorder<PRB>, String, Arc<AtomicBool>)>>>,

    start_delay_secs: f64,
    start_instant: Instant,
}

unsafe impl<PRB: PacketRingBuffer> Send for _AudioProcessWatcher<PRB> {}

impl<PRB: PacketRingBuffer + 'static> _AudioProcessWatcher<PRB> {
    fn new(
        audio_codec: AudioCodec,
        include_tree: bool,
        min_secs: u32,
        audio_recorders: Arc<tokio::sync::Mutex<HashMap<u32, (Recorder<PRB>, String, Arc<AtomicBool>)>>>,
        start_delay_secs: f64,
        start_instant: Instant,
    ) -> Result<Self> {
        let _ = unsafe { windows::Win32::System::Com::CoInitializeEx(None, windows::Win32::System::Com::COINIT_MULTITHREADED) };
        let device_enumerator: IMMDeviceEnumerator = unsafe { CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)? };
        let device = unsafe { device_enumerator.GetDefaultAudioEndpoint(eRender, eMultimedia)? };

        let session_manager: IAudioSessionManager2 = unsafe { device.Activate(CLSCTX_ALL, None)? };

        Ok(Self {
            session_manager,

            audio_codec,
            include_tree,
            min_secs,
            audio_recorders,

            start_delay_secs,
            start_instant,
        })
    }

    async unsafe fn try_add_new_process(
        &mut self,
        p_id: u32,
        start_delay_secs: f64,
    ) -> Option<()> {
        let mut audio_recorders = self.audio_recorders.lock().await;
        if audio_recorders.contains_key(&p_id) {
            return None;
        }
        let recorder = create_audio_recorder(&AudioSourceType::WasApiProcess { process_id: p_id, include_tree: self.include_tree }, &self.audio_codec, self.min_secs, start_delay_secs).ok()?;

        let p_name = Self::get_process_name(p_id).unwrap_or("UNKNOWN???".into());

        /*debug_println!*/eprintln!("Added: PID: {p_id}, {p_name}");

        let boo = Arc::new(AtomicBool::new(true));
        audio_recorders.insert(p_id, (recorder, p_name, boo));

        Some(())
    }

    pub async fn start_listening(mut self) -> Result<()> {
        let (add_process_tx, mut add_process_rx) = tokio::sync::mpsc::unbounded_channel();
        let (_remove_process_tx, mut remove_process_rx) = tokio::sync::mpsc::unbounded_channel();

        let r_session_events = Arc::new(Mutex::new(HashMap::new()));
        let session_events = r_session_events.clone();

        let session_handle: WinAudio::IAudioSessionNotification = SessionNotification { add_process_tx, _remove_process_tx, session_events }.into();


        let session_enum = unsafe { self.session_manager.GetSessionEnumerator()? };
        let count = unsafe { session_enum.GetCount()? };

        debug_println!("count: {count}");

        for i in 0..count {
            let session_control = unsafe { session_enum.GetSession(i)? };
            unsafe { session_handle.OnSessionCreated(&session_control)? }
        }

        for (_, (ref mut recorder, _, _)) in self.audio_recorders.lock().await.iter_mut() {
            recorder.start_recording(None);
        }

        let _ = unsafe { &self.session_manager.RegisterSessionNotification(&session_handle) };


        let audio_recorders = self.audio_recorders.clone();
        let min_secs = self.min_secs;

        tokio::spawn(async move {
            while let Some(p_id) = add_process_rx.recv().await {
                let delay = self.start_instant.elapsed().as_secs_f64();
                if let Some(_) = unsafe { self.try_add_new_process(p_id, delay + self.start_delay_secs) }.await {
                    let mut audio_recorders = self.audio_recorders.lock().await;
                    if let Some((ref mut recorder, _, boo)) = audio_recorders.get_mut(&p_id) {
                        recorder.start_recording(Some(boo.clone()));
                    } else {
                        debug_println!("Recorder removed again :(")
                    }
                }
            }
        });

        tokio::spawn(async move {
            let audio_recorders = audio_recorders;
            let r_session_events = r_session_events;
            while let Some(p_id) = remove_process_rx.recv().await {
                let audio_recorders = audio_recorders.clone();
                let r_session_events = r_session_events.clone();
                tokio::spawn(async move {
                    tokio::time::sleep(Duration::from_secs(min_secs as u64)).await;
                    let mut audio_recorders = audio_recorders.lock().await;
                    if let Some((_, _, boo)) = audio_recorders.remove(&p_id) {
                        boo.store(false, Ordering::Relaxed);
                        /*debug_println!*/eprintln!("removed process {p_id}");
                    } else {
                        debug_println!("did NOT removed process {p_id}");
                    }

                    let mut r_session_events = r_session_events.lock().unwrap();
                    let _ = r_session_events.remove(&p_id);
                });
            }
        });


        Ok(())
    }

    fn get_process_name(pid: u32) -> Option<String> {
        unsafe {
            let snapshot = windows::Win32::System::Diagnostics::ToolHelp::CreateToolhelp32Snapshot(windows::Win32::System::Diagnostics::ToolHelp::TH32CS_SNAPPROCESS, 0).ok()?;
            let mut entry = windows::Win32::System::Diagnostics::ToolHelp::PROCESSENTRY32::default();
            entry.dwSize = size_of::<windows::Win32::System::Diagnostics::ToolHelp::PROCESSENTRY32>() as u32;

            if let Ok(_) = windows::Win32::System::Diagnostics::ToolHelp::Process32First(snapshot, &mut entry) {
                loop {
                    if entry.th32ProcessID == pid {
                        // Convert [i8] to CStr, then to Rust String
                        let cstr = core::ffi::CStr::from_ptr(entry.szExeFile.as_ptr());
                        return Some(cstr.to_string_lossy().into_owned());
                    }
                    if let Err(_) = windows::Win32::System::Diagnostics::ToolHelp::Process32Next(snapshot, &mut entry) {
                        break;
                    }
                }
            }
            None
        }
    }
}

#[windows_core::implement(WinAudio::IAudioSessionNotification)]
struct SessionNotification {
    add_process_tx: tokio::sync::mpsc::UnboundedSender<u32>,
    _remove_process_tx: tokio::sync::mpsc::UnboundedSender<u32>,

    session_events: Arc<Mutex<HashMap<u32, (MaybeSafeComWrapper<WinAudio::IAudioSessionEvents>, MaybeSafeComWrapper<IAudioSessionControl2>)>>>,
}

#[allow(non_snake_case)]
impl WinAudio::IAudioSessionNotification_Impl for SessionNotification_Impl {
    fn OnSessionCreated(&self, newsession: windows::core::Ref<'_, WinAudio::IAudioSessionControl>) -> windows_core::Result<()> {
        if let Some(new_session) = newsession.as_ref() {
            let new_session2: IAudioSessionControl2 = new_session.cast()?;
            let p_id = unsafe { new_session2.GetProcessId()? };

            debug_println!("OnSessionCreated NEW P_ID: {}", p_id);

            let tx = self._remove_process_tx.clone();
            let session_events: WinAudio::IAudioSessionEvents = SessionEvents { p_id, tx }.into();
            let _ = unsafe { new_session.RegisterAudioSessionNotification(&session_events) };

            self.session_events.lock().unwrap().insert(p_id, (MaybeSafeComWrapper(session_events), MaybeSafeComWrapper(new_session2)));

            let _ = self.add_process_tx.send(p_id);
        }
        Ok(())
    }
}

#[windows_core::implement(WinAudio::IAudioSessionEvents)]
pub struct SessionEvents {
    p_id: u32,
    tx: tokio::sync::mpsc::UnboundedSender<u32>,
}

impl WinAudio::IAudioSessionEvents_Impl for SessionEvents_Impl {
    fn OnStateChanged(&self, newstate: AudioSessionState) -> windows_core::Result<()> {
        debug_println!("OnStateChanged, p_id: {}", self.p_id);
        if newstate == AudioSessionStateExpired {
            let _ = self.tx.send(self.p_id);
        }
        Ok(())
    }

    fn OnSessionDisconnected(&self, _disconnectreason: AudioSessionDisconnectReason) -> windows_core::Result<()> {
        debug_println!("OnSessionDisconnected, p_id: {}", self.p_id);
        let _ = self.tx.send(self.p_id);
        Ok(())
    }

    fn OnDisplayNameChanged(&self, _newdisplayname: &PCWSTR, _eventcontext: *const GUID) -> windows_core::Result<()> {
        debug_println!("OnDisplayNameChanged, p_id: {}", self.p_id);
        Ok(())
    }

    fn OnGroupingParamChanged(&self, _newgroupingparam: *const GUID, _eventcontext: *const GUID) -> windows_core::Result<()> {
        debug_println!("OnGroupingParamChanged, p_id: {}", self.p_id);
        Ok(())
    }

    fn OnIconPathChanged(&self, _newiconpath: &PCWSTR, _eventcontext: *const GUID) -> windows_core::Result<()> {
        debug_println!("OnIconPathChanged, p_id: {}", self.p_id);
        Ok(())
    }

    fn OnSimpleVolumeChanged(&self, _newvolume: f32, _newmute: BOOL, _eventcontext: *const GUID) -> windows_core::Result<()> {
        debug_println!("OnSimpleVolumeChanged, p_id: {}", self.p_id);
        Ok(())
    }

    fn OnChannelVolumeChanged(&self, _channelcount: u32, _newchannelvolumearray: *const f32, _changedchannel: u32, _eventcontext: *const GUID) -> windows_core::Result<()> {
        debug_println!("OnChannelVolumeChanged, p_id: {}", self.p_id);
        Ok(())
    }
}


fn new_waveformatextensible(
    storebits: usize,
    validbits: usize,
    //sample_type: SampleType,
    samplerate: usize,
    channels: usize,
    channel_mask: Option<u32>,
) -> WAVEFORMATEXTENSIBLE {
    let blockalign = channels * storebits / 8;
    let byterate = samplerate * blockalign;

    let wave_format = WAVEFORMATEX {
        cbSize: 22,
        nAvgBytesPerSec: byterate as u32,
        nBlockAlign: blockalign as u16,
        nChannels: channels as u16,
        nSamplesPerSec: samplerate as u32,
        wBitsPerSample: storebits as u16,
        wFormatTag: windows::Win32::Media::KernelStreaming::WAVE_FORMAT_EXTENSIBLE as u16,
    };
    let sample = WAVEFORMATEXTENSIBLE_0 {
        wValidBitsPerSample: validbits as u16,
    };
    let subformat = windows::Win32::Media::Multimedia::KSDATAFORMAT_SUBTYPE_IEEE_FLOAT; /*match sample_type {
        Float => windows::Win32::media::Multimedia::KSDATAFORMAT_SUBTYPE_IEEE_FLOAT,
        Int => windows::Win32::media::KernelStreaming::KSDATAFORMAT_SUBTYPE_PCM,
    };*/
    // Only max 18 mask channel positions are defined,
    // https://docs.microsoft.com/en-us/windows/win32/api/mmreg/ns-mmreg-waveformatextensible
    let mask = if let Some(given_mask) = channel_mask {
        given_mask
    } else {
        match channels {
            ch if ch <= 18 => {
                // setting bit for each channel
                (1 << ch) - 1
            }
            _ => 0,
        }
    };

    WAVEFORMATEXTENSIBLE {
        Format: wave_format,
        Samples: sample,
        SubFormat: subformat,
        dwChannelMask: mask,
    }
}


#[windows_core::implement(WinAudio::IActivateAudioInterfaceCompletionHandler)]
struct Handler {
    pair: Arc<(Mutex<bool>, Condvar)>,
}

#[allow(non_snake_case)]
impl WinAudio::IActivateAudioInterfaceCompletionHandler_Impl for Handler_Impl {
    fn ActivateCompleted(&self, _operation: windows::core::Ref<'_, WinAudio::IActivateAudioInterfaceAsyncOperation>) -> windows::core::Result<()> {
        let (lock, cvar) = &*self.pair;
        let mut guard = lock.lock().unwrap();
        *guard = true;
        cvar.notify_all();

        Ok(())
    }
}