use anyhow::Result;
use std::collections::HashMap;
use std::collections::VecDeque;
use std::mem::ManuallyDrop;
use std::ops::Deref;
use std::pin::Pin;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::sync::Condvar;
use std::sync::Mutex;
use std::time::Duration;
use std::time::Instant;

use ffmpeg_next::encoder::audio::Encoder;
use ffmpeg_next::frame::Audio;
use windows::core::IUnknown;
use windows::core::Interface;
use windows::core::HRESULT;
use windows::Win32::Media::Audio as WinAudio;
use windows::Win32::Media::Audio::eCapture;
use windows::Win32::Media::Audio::eConsole;
use windows::Win32::Media::Audio::eMultimedia;
use windows::Win32::Media::Audio::eRender;
use windows::Win32::Media::Audio::AudioSessionDisconnectReason;
use windows::Win32::Media::Audio::AudioSessionState;
use windows::Win32::Media::Audio::AudioSessionStateExpired;
use windows::Win32::Media::Audio::IAudioCaptureClient;
use windows::Win32::Media::Audio::IAudioClient;
use windows::Win32::Media::Audio::IAudioSessionControl2;
use windows::Win32::Media::Audio::IAudioSessionManager2;
use windows::Win32::Media::Audio::IMMDeviceEnumerator;
use windows::Win32::Media::Audio::MMDeviceEnumerator;
use windows::Win32::Media::Audio::AUDCLNT_SHAREMODE_SHARED;
use windows::Win32::Media::Audio::AUDCLNT_STREAMFLAGS_EVENTCALLBACK;
use windows::Win32::Media::Audio::AUDCLNT_STREAMFLAGS_LOOPBACK;
use windows::Win32::Media::Audio::AUDIOCLIENT_ACTIVATION_PARAMS;
use windows::Win32::Media::Audio::AUDIOCLIENT_ACTIVATION_PARAMS_0;
use windows::Win32::Media::Audio::AUDIOCLIENT_ACTIVATION_TYPE_PROCESS_LOOPBACK;
use windows::Win32::Media::Audio::AUDIOCLIENT_PROCESS_LOOPBACK_PARAMS;
use windows::Win32::Media::Audio::PROCESS_LOOPBACK_MODE_EXCLUDE_TARGET_PROCESS_TREE;
use windows::Win32::Media::Audio::PROCESS_LOOPBACK_MODE_INCLUDE_TARGET_PROCESS_TREE;
use windows::Win32::Media::Audio::WAVEFORMATEX;
use windows::Win32::Media::Audio::WAVEFORMATEXTENSIBLE;
use windows::Win32::Media::Audio::WAVEFORMATEXTENSIBLE_0;
use windows::Win32::System::Com::CoCreateInstance;
use windows::Win32::System::Com::StructuredStorage::PROPVARIANT;
use windows::Win32::System::Com::StructuredStorage::PROPVARIANT_0;
use windows::Win32::System::Com::StructuredStorage::PROPVARIANT_0_0;
use windows::Win32::System::Com::StructuredStorage::PROPVARIANT_0_0_0;
use windows::Win32::System::Com::BLOB;
use windows::Win32::System::Com::CLSCTX_ALL;
use windows::Win32::System::Performance::QueryPerformanceCounter;
use windows::Win32::System::Performance::QueryPerformanceFrequency;
use windows::Win32::System::Threading::CreateEventW;
use windows::Win32::System::Threading::WaitForSingleObject;
use windows::Win32::System::Threading::INFINITE;
use windows::Win32::System::Variant::VT_BLOB;
use windows_core::BOOL;
use windows_core::GUID;
use windows_core::PCWSTR;

pub struct EnvWasapi {
    client: IAudioClient,
    format: WAVEFORMATEXTENSIBLE,
}

impl EnvWasapi {
    pub fn new(
        process_id: Option<u32>,
        render_else_capture: bool,
        include_process_tree: bool,
    ) -> Result<Self> {
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

        Ok(Self {
            client,
            format,
        })
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
    }
        .context("failed to create MMDeviceEnumerator")?;

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
    let mut activation_params = AUDIOCLIENT_ACTIVATION_PARAMS {
        ActivationType: AUDIOCLIENT_ACTIVATION_TYPE_PROCESS_LOOPBACK,

        Anonymous: AUDIOCLIENT_ACTIVATION_PARAMS_0 {
            ProcessLoopbackParams:
            AUDIOCLIENT_PROCESS_LOOPBACK_PARAMS {
                TargetProcessId: process_id,

                ProcessLoopbackMode: if include_process_tree {
                    PROCESS_LOOPBACK_MODE_INCLUDE_TARGET_PROCESS_TREE
                } else {
                    PROCESS_LOOPBACK_MODE_EXCLUDE_TARGET_PROCESS_TREE
                },
            },
        },
    };

    let activation_propvariant = PROPVARIANT {
        Anonymous: PROPVARIANT_0 {
            Anonymous: ManuallyDrop::new(
                PROPVARIANT_0_0 {
                    vt: VT_BLOB,
                    wReserved1: 0,
                    wReserved2: 0,
                    wReserved3: 0,

                    Anonymous: PROPVARIANT_0_0_0 {
                        blob: BLOB {
                            cbSize: size_of::<
                                AUDIOCLIENT_ACTIVATION_PARAMS,
                            >() as u32,

                            pBlobData: (
                                &mut activation_params
                                    as *mut AUDIOCLIENT_ACTIVATION_PARAMS
                            )
                                .cast::<u8>(),
                        },
                    },
                },
            ),
        },
    };

    let completion = Arc::new((
        Mutex::new(false),
        Condvar::new(),
    ));

    let handler: WinAudio::IActivateAudioInterfaceCompletionHandler =
        Handler {
            pair: completion.clone(),
        }
            .into();

    let operation = unsafe {
        WinAudio::ActivateAudioInterfaceAsync(
            WinAudio::VIRTUAL_AUDIO_DEVICE_PROCESS_LOOPBACK,
            &IAudioClient::IID,
            Some(&activation_propvariant),
            &handler,
        )
    }
        .context("failed to start process-loopback activation")?;

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
            anyhow!(
                "process-loopback activation returned no interface"
            )
        })?
        .cast()
        .context("failed to cast interface to IAudioClient")?;

    let format = get_owned_mix_format(&client)?;

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

    Ok((client, format))
}

fn get_owned_mix_format(
    client: &IAudioClient,
) -> Result<WAVEFORMATEXTENSIBLE> {
    let format_ptr = unsafe {
        client.GetMixFormat()?
    };

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
            format.Format.wFormatTag
        );
    }

    Ok(format)
}