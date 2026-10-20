use std::mem::ManuallyDrop;
use std::sync::Arc;
use std::sync::Condvar;
use std::sync::Mutex;
use std::time::Duration;

use anyhow::{anyhow, bail, Context, Result};
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
use windows::Win32::Media::Audio::eCapture;
use windows::Win32::Media::Audio::eConsole;
use windows::Win32::Media::Audio::eRender;
use windows::Win32::Media::Audio::IAudioClient;
use windows::Win32::Media::Audio::IMMDeviceEnumerator;
use windows::Win32::Media::Audio::MMDeviceEnumerator;
use windows::Win32::Media::Audio::PROCESS_LOOPBACK_MODE_EXCLUDE_TARGET_PROCESS_TREE;
use windows::Win32::Media::Audio::PROCESS_LOOPBACK_MODE_INCLUDE_TARGET_PROCESS_TREE;
use windows::Win32::Media::Audio::WAVEFORMATEX;
use windows::Win32::Media::Audio::WAVEFORMATEXTENSIBLE;
use windows::Win32::Media::Audio::WAVEFORMATEXTENSIBLE_0;
use windows::Win32::Media::KernelStreaming::{KSDATAFORMAT_SUBTYPE_PCM, WAVE_FORMAT_EXTENSIBLE};
use windows::Win32::Media::Multimedia::KSDATAFORMAT_SUBTYPE_IEEE_FLOAT;
use windows::Win32::System::Com::{CoCreateInstance, CoTaskMemAlloc, CoTaskMemFree};
use windows::Win32::System::Com::BLOB;
use windows::Win32::System::Com::CLSCTX_ALL;
use windows::Win32::System::Com::StructuredStorage::PROPVARIANT;
use windows::Win32::System::Com::StructuredStorage::PROPVARIANT_0;
use windows::Win32::System::Com::StructuredStorage::PROPVARIANT_0_0;
use windows::Win32::System::Com::StructuredStorage::PROPVARIANT_0_0_0;
use windows::Win32::System::Threading::CreateEventW;
use windows::Win32::System::Threading::INFINITE;
use windows::Win32::System::Threading::WaitForSingleObject;
use windows::Win32::System::Variant::VT_BLOB;

use crate::recorders::traits::Event;

pub struct EnvWasapi {
    pub client: IAudioClient,
    pub format: WaveFormat,
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

pub struct WaveFormat {
    ptr: *mut WAVEFORMATEX,
}

impl WaveFormat {
    pub fn new(client: &IAudioClient) -> Result<Self> {
        let ptr = unsafe { client.GetMixFormat()? };
        Ok(Self { ptr })
    }

    pub fn new_extensible(
        storebits: u16,
        validbits: u16,
        sample_type: SampleType,
        samplerate: u32,
        channels: u16,
        channel_mask: Option<u32>,
    ) -> Result<Self> {
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
            1 => 0x4,
            2 => 0x3,
            6 => 0x3F,
            8 => 0x63F,
            _ => 0,
        });

        anyhow::ensure!(
            mask == 0 || mask.count_ones() == u32::from(channels),
            "Channel mask doesn't match channel count"
        );

        let format = WAVEFORMATEXTENSIBLE {
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
        };

        let ptr = unsafe {
            CoTaskMemAlloc(size_of::<WAVEFORMATEXTENSIBLE>())
                as *mut WAVEFORMATEXTENSIBLE
        };

        anyhow::ensure!(!ptr.is_null(), "CoTaskMemAlloc failed");

        unsafe {
            ptr.write(format);
        }

        Ok(Self {
            ptr: ptr.cast::<WAVEFORMATEX>(),
        })
    }

    pub fn format(&self) -> &WAVEFORMATEX {
        unsafe { &*self.ptr }
    }

    pub fn extensible(&self) -> Option<&WAVEFORMATEXTENSIBLE> {
        let format = self.format();

        if format.wFormatTag != WAVE_FORMAT_EXTENSIBLE as u16 || format.cbSize < 22
        {
            return None;
        }

        Some(unsafe {
            &*(self.ptr as *const WAVEFORMATEXTENSIBLE)
        })
    }
}

impl Drop for WaveFormat {
    fn drop(&mut self) {
        unsafe {
            CoTaskMemFree(Some(self.ptr.cast()));
        }
    }
}

fn create_default_iaudioclient(
    render_else_capture: bool,
) -> Result<(IAudioClient, WaveFormat)> {
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
    }.context("failed to get default audio endpoint")?;

    let client: IAudioClient = unsafe {
        device.Activate(
            CLSCTX_ALL,
            None,
        )
    }.context("failed to activate IAudioClient")?;

    let format = WaveFormat::new(&client)?;

    unsafe {
        client.Initialize(
            AUDCLNT_SHAREMODE_SHARED,
            stream_flags,
            0,
            0,
            format.format(),
            None,
        )
    }.context("failed to initialize default IAudioClient")?;

    Ok((client, format))
}

pub fn create_process_iaudioclient(
    process_id: u32,
    include_process_tree: bool,
) -> Result<(IAudioClient, WaveFormat)> {

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

    let completion = Arc::new((
        Mutex::new(false),
        Condvar::new(),
    ));

    let handler: WinAudio::IActivateAudioInterfaceCompletionHandler =Handler {
        pair: completion.clone(),
    }.into();

    let operation = unsafe {
        WinAudio::ActivateAudioInterfaceAsync(
            WinAudio::VIRTUAL_AUDIO_DEVICE_PROCESS_LOOPBACK,
            &IAudioClient::IID,
            Some(&*activation_propvariant),
            &handler,
        )
    }.context("failed to start process-loopback activation")?;

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

    let mut activation_result = HRESULT(0);
    let mut activated_interface: Option<IUnknown> = None;

    unsafe {
        operation.GetActivateResult(
            &mut activation_result,
            &mut activated_interface,
        )?;
    }

    activation_result
        .ok()
        .context("process-loopback activation failed")?;

    let client: IAudioClient = activated_interface
        .ok_or_else(|| {
            anyhow!("process-loopback activation returned no interface")
        })?
        .cast()
        .context("failed to cast interface to IAudioClient")?;

    let format = WaveFormat::new_extensible(
        32,
        32,
        SampleType::Float,
        48000,
        2,
        None,
    )?;

    unsafe {
        client.Initialize(
            AUDCLNT_SHAREMODE_SHARED,
            AUDCLNT_STREAMFLAGS_LOOPBACK
                | AUDCLNT_STREAMFLAGS_EVENTCALLBACK,
            0,
            0,
            format.format(),
            None,
        )
    }.context("failed to initialize process-loopback IAudioClient")?;

    Ok((client, format))
}

#[derive(Clone, Copy)]
#[allow(dead_code)]
pub enum SampleType {
    Float,
    Pcm,
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
