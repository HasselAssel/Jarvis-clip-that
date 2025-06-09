mod capturer;
mod com;


use std::collections::HashMap;
use std::os::windows::ffi::OsStringExt;
use windows::{
    Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Process32FirstW, Process32NextW,
        PROCESSENTRY32W, TH32CS_SNAPPROCESS,
    },
    Win32::System::Threading::{OpenProcess, PROCESS_ACCESS_RIGHTS},
    Win32::Foundation::{HANDLE, INVALID_HANDLE_VALUE},
};
use windows::Win32::Foundation::CloseHandle;
use windows::Win32::Media::Audio::{eConsole, eRender, IAudioSessionControl, IAudioSessionControl2, IAudioSessionManager2, IMMDeviceEnumerator, MMDeviceEnumerator};
use windows::Win32::System::Com::CoCreateInstance;
use windows_core::Interface;

use crate::capturer::capture::capturer::MainCapturer;

fn main() {
    let pids = get_all_pids().unwrap();
    println!("Running PIDs: {:?}", pids);

    unsafe {
        windows::Win32::System::Com::CoInitializeEx(None, windows::Win32::System::Com::COINIT_MULTITHREADED).unwrap();

        let enumerator: IMMDeviceEnumerator =
            CoCreateInstance(
                &MMDeviceEnumerator,
                None,
                windows::Win32::System::Com::CLSCTX_ALL,
            ).unwrap();

        let device =
            enumerator.GetDefaultAudioEndpoint(
                eRender,
                eConsole,
            ).unwrap();

        let session_mgr: IAudioSessionManager2 = device.Activate(windows::Win32::System::Com::CLSCTX_ALL, None).unwrap();

        let session_enum = session_mgr.GetSessionEnumerator().unwrap();

        let count = session_enum.GetCount().unwrap();
        println!("Count: {}", count);

        for i in 0..count {
            let session_control = session_enum.GetSession(i).unwrap();

            let session_control2: IAudioSessionControl2 = session_control.cast().unwrap();
            let pid = unsafe { session_control2.GetProcessId().unwrap() };
            println!("Session {} belongs to PID: {}, Name: {}", i, pid, pids.get(&pid).unwrap_or(&"UNKNOWN".to_string()));
        }
    }

    ffmpeg_next::init().unwrap();
    //ffmpeg_next::log::set_level(ffmpeg_next::log::Level::Debug);
    let mut cap = MainCapturer::new();
    cap.start_capturing();
}


fn get_all_pids() -> Result<HashMap<u32, String>, windows::core::Error> {
    unsafe {
        // Create a snapshot of all running processes
        let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0)?;
        if snapshot == INVALID_HANDLE_VALUE {
            return Err(windows::core::Error::from_win32());
        }

        let mut proc_entry: PROCESSENTRY32W = std::mem::zeroed();
        proc_entry.dwSize = size_of::<PROCESSENTRY32W>() as u32;

        // Initialize iteration
        if Process32FirstW(snapshot, &mut proc_entry).is_err() {
            CloseHandle(snapshot)?;
            return Err(windows::core::Error::from_win32());
        }

        let mut pids = HashMap::new();
        loop {
            // Collect the PID from the current entry
            let pid = proc_entry.th32ProcessID;
            let exe_name = std::ffi::OsString::from_wide(
                &proc_entry.szExeFile
                    .iter()
                    .take_while(|&&c| c != 0)
                    .cloned()
                    .collect::<Vec<u16>>()
            ).to_string_lossy().into_owned();
            pids.insert(pid, exe_name);

            // Advance to the next process
            if Process32NextW(snapshot, &mut proc_entry).is_err() {
                break;
            }
        }

        CloseHandle(snapshot)?;
        Ok(pids)
    }
}
