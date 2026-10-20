use std::collections::HashMap;
use std::collections::VecDeque;
use std::mem::ManuallyDrop;
use std::ops::Deref;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;
use std::sync::Condvar;
use std::sync::Mutex;
use std::thread;
use std::time::Duration;
use std::time::Instant;

use anyhow::{anyhow, bail, Context, Result};
use ffmpeg_next::encoder::audio::Encoder;
use ffmpeg_next::frame::Audio;
use windows::core::HRESULT;
use windows::core::Interface;
use windows::core::IUnknown;
use windows::Win32::Foundation::HANDLE;
use windows::Win32::Media::Audio as WinAudio;
use windows::Win32::Media::Audio::AUDCLNT_SHAREMODE_SHARED;
use windows::Win32::Media::Audio::AUDCLNT_STREAMFLAGS_EVENTCALLBACK;
use windows::Win32::Media::Audio::AUDCLNT_STREAMFLAGS_LOOPBACK;
use windows::Win32::Media::Audio::AUDIOCLIENT_ACTIVATION_PARAMS;
use windows::Win32::Media::Audio::AUDIOCLIENT_ACTIVATION_PARAMS_0;
use windows::Win32::Media::Audio::AUDIOCLIENT_ACTIVATION_TYPE_PROCESS_LOOPBACK;
use windows::Win32::Media::Audio::AUDIOCLIENT_PROCESS_LOOPBACK_PARAMS;
use windows::Win32::Media::Audio::AudioSessionDisconnectReason;
use windows::Win32::Media::Audio::AudioSessionState;
use windows::Win32::Media::Audio::AudioSessionStateExpired;
use windows::Win32::Media::Audio::eCapture;
use windows::Win32::Media::Audio::eConsole;
use windows::Win32::Media::Audio::eMultimedia;
use windows::Win32::Media::Audio::eRender;
use windows::Win32::Media::Audio::IAudioCaptureClient;
use windows::Win32::Media::Audio::IAudioClient;
use windows::Win32::Media::Audio::IAudioSessionControl2;
use windows::Win32::Media::Audio::IAudioSessionManager2;
use windows::Win32::Media::Audio::IMMDeviceEnumerator;
use windows::Win32::Media::Audio::MMDeviceEnumerator;
use windows::Win32::Media::Audio::PROCESS_LOOPBACK_MODE_EXCLUDE_TARGET_PROCESS_TREE;
use windows::Win32::Media::Audio::PROCESS_LOOPBACK_MODE_INCLUDE_TARGET_PROCESS_TREE;
use windows::Win32::Media::Audio::WAVEFORMATEX;
use windows::Win32::Media::Audio::WAVEFORMATEXTENSIBLE;
use windows::Win32::Media::Audio::WAVEFORMATEXTENSIBLE_0;
use windows::Win32::Media::KernelStreaming::{KSDATAFORMAT_SUBTYPE_PCM, WAVE_FORMAT_EXTENSIBLE};
use windows::Win32::Media::Multimedia::KSDATAFORMAT_SUBTYPE_IEEE_FLOAT;
use windows::Win32::System::Com::{CoCreateInstance, CoTaskMemFree};
use windows::Win32::System::Com::BLOB;
use windows::Win32::System::Com::CLSCTX_ALL;
use windows::Win32::System::Com::StructuredStorage::PROPVARIANT;
use windows::Win32::System::Com::StructuredStorage::PROPVARIANT_0;
use windows::Win32::System::Com::StructuredStorage::PROPVARIANT_0_0;
use windows::Win32::System::Com::StructuredStorage::PROPVARIANT_0_0_0;
use windows::Win32::System::Performance::QueryPerformanceCounter;
use windows::Win32::System::Performance::QueryPerformanceFrequency;
use windows::Win32::System::Threading::CreateEventW;
use windows::Win32::System::Threading::INFINITE;
use windows::Win32::System::Threading::WaitForSingleObject;
use windows::Win32::System::Variant::VT_BLOB;
use windows_core::BOOL;
use windows_core::GUID;
use windows_core::PCWSTR;

use crate::debug;
use crate::recorders::traits::Event;
use crate::recorders::windows::wasapi::sources::SourceWasapi;

pub struct EnvWasapi {
    pub client: IAudioClient,
    pub format: WAVEFORMATEXTENSIBLE,
    event: HANDLE,
}

unsafe impl Send for EnvWasapi {} // TODO: Yea I am 67% sure this isn't safe. Change architecture to create Env on destination thread!

impl EnvWasapi {
    pub fn new(
        process_id: Option<u32>,
        render_else_capture: bool,
        include_process_tree: bool,
    ) -> Result<Self> {
        let _ = ensure_co_initialize();

        debug!("bef");
        let (client, format) = match process_id {
            Some(process_id) => {
                create_process_iaudioclient(
                    process_id,
                    include_process_tree,
                )?
            }

            None => {
                create_default_iaudioclient(
                    render_else_capture,
                )?
            }
        };
        debug!("aft");

        let event;
        unsafe {
            event = CreateEventW(None, false, false, None)?;
            client.SetEventHandle(event)?;
        }

        Ok(Self {
            client,
            format,
            event,
        })
    }
}

impl Event for EnvWasapi {
    fn start(&mut self) {
        unsafe { self.client.Start().unwrap(); }
        self.await_event();
    }
    fn await_event(&mut self) {
        unsafe {
            let _ = WaitForSingleObject(self.event, INFINITE);
        }
        // TODO: handle return codes!!!
    }
}

pub fn ensure_co_initialize() -> HRESULT {
    unsafe {
        windows::Win32::System::Com::CoInitializeEx(
            None,
            windows::Win32::System::Com::COINIT_MULTITHREADED,
        )
    }
}

fn create_default_iaudioclient(
    render_else_capture: bool,
) -> Result<(IAudioClient, WAVEFORMATEXTENSIBLE)> {
    let enumerator: IMMDeviceEnumerator = unsafe {
        CoCreateInstance(
            &MMDeviceEnumerator,
            None,
            CLSCTX_ALL,
        )
    }.context("failed to create MMDeviceEnumerator")?;

    let data_flow = if render_else_capture {
        eRender
    } else {
        eCapture
    };

    let stream_flags = if render_else_capture {
        AUDCLNT_STREAMFLAGS_LOOPBACK
            | AUDCLNT_STREAMFLAGS_EVENTCALLBACK
    } else {
        AUDCLNT_STREAMFLAGS_EVENTCALLBACK
    };

    let device = unsafe {
        enumerator.GetDefaultAudioEndpoint(
            data_flow,
            eConsole,
        )
    }
        .context("failed to get default audio endpoint")?;

    let client: IAudioClient = unsafe {
        device.Activate(
            CLSCTX_ALL,
            None,
        )
    }
        .context("failed to activate IAudioClient")?;

    let format = get_owned_mix_format(&client)?;

    unsafe {
        client.Initialize(
            AUDCLNT_SHAREMODE_SHARED,
            stream_flags,
            0,
            0,
            &format.Format,
            None,
        )
    }
        .context("failed to initialize default IAudioClient")?;

    Ok((client, format))
}

pub fn create_process_iaudioclient(
    process_id: u32,
    include_process_tree: bool,
) -> Result<(IAudioClient, WAVEFORMATEXTENSIBLE)> {
    debug!(process_id);

    let mut activation_params = AUDIOCLIENT_ACTIVATION_PARAMS {
        ActivationType: AUDIOCLIENT_ACTIVATION_TYPE_PROCESS_LOOPBACK,
        Anonymous: AUDIOCLIENT_ACTIVATION_PARAMS_0 {
            ProcessLoopbackParams: AUDIOCLIENT_PROCESS_LOOPBACK_PARAMS {
                TargetProcessId: process_id,
                ProcessLoopbackMode: if include_process_tree {
                    PROCESS_LOOPBACK_MODE_INCLUDE_TARGET_PROCESS_TREE
                } else {
                    PROCESS_LOOPBACK_MODE_EXCLUDE_TARGET_PROCESS_TREE
                },
            },
        },
    };

    debug!(process_id);

    let activation_propvariant = ManuallyDrop::new(PROPVARIANT {
        Anonymous: PROPVARIANT_0 {
            Anonymous: ManuallyDrop::new(PROPVARIANT_0_0 {
                vt: VT_BLOB,
                wReserved1: 0,
                wReserved2: 0,
                wReserved3: 0,
                Anonymous: PROPVARIANT_0_0_0 {
                    blob: BLOB {
                        cbSize: size_of::<AUDIOCLIENT_ACTIVATION_PARAMS>() as u32,
                        pBlobData: (&mut activation_params
                            as *mut AUDIOCLIENT_ACTIVATION_PARAMS)
                            .cast::<u8>(),
                    },
                },
            }),
        },
    });

    debug!(process_id);

    let completion = Arc::new((
        Mutex::new(false),
        Condvar::new(),
    ));

    let handler: WinAudio::IActivateAudioInterfaceCompletionHandler =
        Handler {
            pair: completion.clone(),
        }
            .into();

    debug!(process_id);

    let operation = unsafe {
        WinAudio::ActivateAudioInterfaceAsync(
            WinAudio::VIRTUAL_AUDIO_DEVICE_PROCESS_LOOPBACK,
            &IAudioClient::IID,
            Some(&*activation_propvariant),
            &handler,
        )
    }
        .context("failed to start process-loopback activation")?;

    debug!(process_id);

    {
        let (lock, condition_variable) = &*completion;

        let completed = lock
            .lock()
            .map_err(|_| anyhow!("activation mutex was poisoned"))?;

        let (completed, wait_result) = condition_variable
            .wait_timeout_while(
                completed,
                Duration::from_secs(10),
                |completed| !*completed,
            )
            .map_err(|_| anyhow!("activation mutex was poisoned"))?;

        if !*completed && wait_result.timed_out() {
            bail!("process-loopback activation timed out");
        }
    }

    debug!(process_id);

    let mut activation_result = HRESULT(0);
    let mut activated_interface: Option<IUnknown> = None;

    unsafe {
        operation.GetActivateResult(
            &mut activation_result,
            &mut activated_interface,
        )?;
    }

    debug!(process_id);
    debug!(activation_result);

    activation_result
        .ok()
        .context("process-loopback activation failed")?;

    debug!(process_id);

    let client: IAudioClient = activated_interface
        .ok_or_else(|| {
            anyhow!("process-loopback activation returned no interface")
        })?
        .cast()
        .context("failed to cast interface to IAudioClient")?;

    debug!(process_id);

    let format = new_waveformatextensible(
        32,
        32,
        SampleType::Float,
        48000,
        2,
        None,
    )?;

    debug!(process_id);

    unsafe {
        client.Initialize(
            AUDCLNT_SHAREMODE_SHARED,
            AUDCLNT_STREAMFLAGS_LOOPBACK
                | AUDCLNT_STREAMFLAGS_EVENTCALLBACK,
            0,
            0,
            &format.Format,
            None,
        )
    }
        .context("failed to initialize process-loopback IAudioClient")?;

    debug!(process_id);

    thread::sleep(Duration::from_millis(2000));

    debug!(process_id);

    Ok((client, format))
}

fn get_owned_mix_format(
    client: &IAudioClient,
) -> Result<WAVEFORMATEXTENSIBLE> {
    debug!("before");
    let format_ptr = unsafe {
        client.GetMixFormat()?
    };
    debug!("after");

    if format_ptr.is_null() {
        bail!("GetMixFormat returned a null pointer");
    }

    let format = unsafe {
        let format = (
            format_ptr as *const WAVEFORMATEXTENSIBLE
        )
            .read();

        CoTaskMemFree(Some(
            format_ptr.cast::<std::ffi::c_void>(),
        ));

        format
    };

    if format.Format.wFormatTag
        != WAVE_FORMAT_EXTENSIBLE as u16
    {
        bail!(
            "expected WAVE_FORMAT_EXTENSIBLE, got format tag {}",
            {let σ = format.Format.wFormatTag; σ}
        );
    }

    Ok(format)
}

#[derive(Clone, Copy)]
pub enum SampleType {
    Float,
    Pcm,
}

fn new_waveformatextensible(
    storebits: u16,
    validbits: u16,
    sample_type: SampleType,
    samplerate: u32,
    channels: u16,
    channel_mask: Option<u32>,
) -> Result<WAVEFORMATEXTENSIBLE> {
    anyhow::ensure!(channels > 0, "Invalid channel count");
    anyhow::ensure!(samplerate > 0, "Invalid sample rate");
    anyhow::ensure!(
        storebits > 0 && storebits % 8 == 0,
        "Sample container must be byte-aligned"
    );
    anyhow::ensure!(
        validbits > 0 && validbits <= storebits,
        "Invalid validbits"
    );
    anyhow::ensure!(
        !matches!(sample_type, SampleType::Float)
            || (storebits == 32 && validbits == 32),
        "Only 32-bit float is supported"
    );

    let blockalign = u32::from(channels) * u32::from(storebits) / 8;
    let byterate = u64::from(samplerate) * u64::from(blockalign);

    anyhow::ensure!(blockalign <= u16::MAX as u32, "Block align overflow");
    anyhow::ensure!(byterate <= u32::MAX as u64, "Byte rate overflow");

    let mask = channel_mask.unwrap_or(match channels {
        1 => 0x4,   // Front center
        2 => 0x3,   // Front left + right
        6 => 0x3F,  // 5.1
        8 => 0x63F, // 7.1 surround
        _ => 0,     // Unspecified positions
    });

    anyhow::ensure!(
        mask == 0 || mask.count_ones() == u32::from(channels),
        "Channel mask doesn't match channel count"
    );

    Ok(WAVEFORMATEXTENSIBLE {
        Format: WAVEFORMATEX {
            wFormatTag: WAVE_FORMAT_EXTENSIBLE as u16,
            nChannels: channels,
            nSamplesPerSec: samplerate,
            nAvgBytesPerSec: byterate as u32,
            nBlockAlign: blockalign as u16,
            wBitsPerSample: storebits,
            cbSize: 22,
        },
        Samples: WAVEFORMATEXTENSIBLE_0 {
            wValidBitsPerSample: validbits,
        },
        dwChannelMask: mask,
        SubFormat: match sample_type {
            SampleType::Float => KSDATAFORMAT_SUBTYPE_IEEE_FLOAT,
            SampleType::Pcm => KSDATAFORMAT_SUBTYPE_PCM,
        },
    })
}

#[windows_core::implement(WinAudio::IActivateAudioInterfaceCompletionHandler)]
struct Handler {
    pair: Arc<(Mutex<bool>, Condvar)>,
}

#[allow(non_snake_case)]
impl WinAudio::IActivateAudioInterfaceCompletionHandler_Impl for Handler_Impl {
    fn ActivateCompleted(
        &self,
        _operation: windows::core::Ref<'_, WinAudio::IActivateAudioInterfaceAsyncOperation>,
    ) -> windows::core::Result<()> {
        let (lock, cvar) = &*self.pair;
        let mut guard = lock.lock().unwrap();
        *guard = true;
        cvar.notify_all();

        Ok(())
    }
}
