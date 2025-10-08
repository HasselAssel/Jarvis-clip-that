use windows::Win32::Media::Audio::{AUDCLNT_SHAREMODE_SHARED, AUDCLNT_STREAMFLAGS_EVENTCALLBACK, AUDCLNT_STREAMFLAGS_LOOPBACK, eConsole, eRender, IAudioClient, IMMDeviceEnumerator, MMDeviceEnumerator, WAVEFORMATEX};
use windows::Win32::System::Com::CoCreateInstance;

use crate::types::Result;

pub fn create_default_iaudioclient() -> Result<(IAudioClient, WAVEFORMATEX)> {
    let try_init = unsafe {
        windows::Win32::System::Com::CoInitializeEx(None, windows::Win32::System::Com::COINIT_MULTITHREADED)
    };
    if try_init.is_err(){
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