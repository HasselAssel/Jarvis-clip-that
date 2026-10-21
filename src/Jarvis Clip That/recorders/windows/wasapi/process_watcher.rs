use std::collections::HashMap;

use anyhow::Result;
use crossbeam_channel::{Sender, unbounded};
use windows::Win32::Foundation::CloseHandle;
use windows::Win32::Media::Audio::{AudioSessionDisconnectReason, AudioSessionState, AudioSessionStateExpired, eMultimedia, eRender, IAudioSessionControl, IAudioSessionControl2, IAudioSessionManager2, IMMDeviceEnumerator, MMDeviceEnumerator};
use windows::Win32::Media::Audio as WinAudio;
use windows::Win32::System::Com::{CLSCTX_ALL, CoCreateInstance};
use windows::Win32::System::Com::CoTaskMemFree;
use windows::Win32::System::Diagnostics::ToolHelp::{CreateToolhelp32Snapshot, Process32First, PROCESSENTRY32};
use windows::Win32::System::Diagnostics::ToolHelp::Process32Next;
use windows_core::{BOOL, GUID, Interface, PCWSTR, Ref};

use crate::debug;
use crate::recorders::config::{AudioCodec, AudioConfig, Config, FrameConfig};
use crate::recorders::factory::FactoryCommands;
use crate::recorders::windows::{AudioEnvironment, WasAPI};
use crate::recorders::windows::wasapi::env::ensure_co_initialize;

pub struct WasApiProcessWatcher {
    session_manager: IAudioSessionManager2,
    cmd_send: Sender<FactoryCommands>,

    codec: AudioCodec,
    include_tree: bool,

    session_events: HashMap<u32, HashMap<String, SessionData>>,
}

impl WasApiProcessWatcher {
    pub fn new(
        codec: AudioCodec,
        include_tree: bool,
        cmd_send: Sender<FactoryCommands>,
    ) -> Result<Self> {
        let _ = ensure_co_initialize();

        let device_enumerator: IMMDeviceEnumerator = unsafe {
            CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?
        };
        let device = unsafe { device_enumerator.GetDefaultAudioEndpoint(eRender, eMultimedia)? };

        let session_manager: IAudioSessionManager2 = unsafe { device.Activate(CLSCTX_ALL, None)? };

        Ok(Self {
            session_manager,
            cmd_send,

            codec,
            include_tree,

            session_events: HashMap::new(),
        })
    }

    fn get_process_name(pid: u32) -> Option<String> {
        unsafe {
            let snapshot = CreateToolhelp32Snapshot(
                windows::Win32::System::Diagnostics::ToolHelp::TH32CS_SNAPPROCESS,
                0,
            ).ok()?;

            let mut entry = PROCESSENTRY32::default();
            entry.dwSize = size_of::<PROCESSENTRY32>() as u32;

            if let Ok(_) = Process32First(snapshot, &mut entry)
            {
                loop {
                    if entry.th32ProcessID == pid {
                        // Convert [i8] to CStr, then to Rust String
                        let cstr = core::ffi::CStr::from_ptr(entry.szExeFile.as_ptr());
                        CloseHandle(snapshot).ok()?;
                        return Some(cstr.to_string_lossy().into_owned());
                    }
                    if let Err(_) = Process32Next(
                        snapshot, &mut entry,
                    ) {
                        break;
                    }
                }
            }
            CloseHandle(snapshot).ok()?;
            None
        }
    }

    fn config(&self, p_id: u32) -> Config {
        Config {
            frame_config: FrameConfig::Audio(AudioConfig {
                environment: AudioEnvironment::WasApi(WasAPI::WasApiProcess {
                    process_id: p_id,
                    include_tree: self.include_tree,
                }),
                codec: self.codec.clone(),
            }),
            name: Self::get_process_name(p_id).unwrap_or_else(|| String::new()),
        }
    }

    pub fn start(mut self) -> Result<()> {
        let (sender, receiver) = unbounded();

        let session_notification: WinAudio::IAudioSessionNotification = SessionNotification {
            sender
        }.into();

        unsafe {
            self.session_manager.RegisterSessionNotification(&session_notification)?
        };

        let session_enum = unsafe { self.session_manager.GetSessionEnumerator()? };
        let count = unsafe { session_enum.GetCount()? };
        for i in 0..count {
            let session_control = unsafe { session_enum.GetSession(i)? };
            unsafe { session_notification.OnSessionCreated(&session_control)? }
        }

        while let Ok(cmd) = receiver.recv() {
            match cmd {
                Command::AddProcess(session_id_str, p_id, session_data) => {
                    debug!(&session_id_str, p_id);
                    let config = self.config(p_id);
                    let map = self.session_events
                        .entry(p_id)
                        .or_insert(HashMap::new());
                    if map.is_empty() {
                        self.cmd_send.send(FactoryCommands::AddRecorder(config))?;
                    }
                    map.insert(session_id_str, session_data);
                },
                Command::RemoveProcess(session_id_str, p_id) => {
                    debug!(&session_id_str, p_id);
                    let map = self.session_events
                        .entry(p_id)
                        .or_insert(HashMap::new());
                    map.remove(&session_id_str);
                    if map.is_empty() {
                        //self.cmd_send.send()
                        // TODO: remove this process recorder.
                    }
                }
            }
        }

        unsafe {
            self.session_manager.UnregisterSessionNotification(&session_notification)?;
        }
 
        Ok(())
    }
}

enum Command {
    AddProcess(String, u32, SessionData),
    RemoveProcess(String, u32),
}

#[windows_core::implement(WinAudio::IAudioSessionNotification)]
struct SessionNotification {
    sender: Sender<Command>,
}

#[allow(non_snake_case)]
impl WinAudio::IAudioSessionNotification_Impl for SessionNotification_Impl {
    fn OnSessionCreated(&self, newsession: Ref<'_, IAudioSessionControl>) -> windows_core::Result<()> {
        if let Some(new_session) = newsession.as_ref() {
            let new_session2: IAudioSessionControl2 = new_session.cast()?;

            let p_id;
            let id;
            let session_id_str;
            unsafe {
                p_id = new_session2.GetProcessId()?;
                id = new_session2.GetSessionInstanceIdentifier()?;
                let session_id_str_res = id.to_string();
                CoTaskMemFree(Some(id.0.cast()));
                session_id_str = session_id_str_res?
            }

            let session_events: WinAudio::IAudioSessionEvents = SessionEvent {
                session_id_str: session_id_str.clone(),
                p_id,
                sender: self.sender.clone()
            }.into();

            unsafe { new_session.RegisterAudioSessionNotification(&session_events) }?;

            let session_data = SessionData {
                event: session_events,
                control: new_session2,
            };

            let _ = self.sender.send(Command::AddProcess(session_id_str, p_id, session_data));
        }
        Ok(())
    }
}

#[windows_core::implement(WinAudio::IAudioSessionEvents)]
struct SessionEvent {
    session_id_str: String,
    p_id: u32,
    sender: Sender<Command>,
}

impl WinAudio::IAudioSessionEvents_Impl for SessionEvent_Impl {
    fn OnStateChanged(&self, newstate: AudioSessionState) -> windows_core::Result<()> {
        debug!(self.p_id);
        if newstate == AudioSessionStateExpired {
            let _ = self.sender.send(Command::RemoveProcess(self.session_id_str.clone(), self.p_id));
        }
        Ok(())
    }

    fn OnSessionDisconnected(
        &self,
        _disconnectreason: AudioSessionDisconnectReason,
    ) -> windows_core::Result<()> {
        debug!(self.p_id);
        let _ = self.sender.send(Command::RemoveProcess(self.session_id_str.clone(), self.p_id));
        Ok(())
    }

    fn OnDisplayNameChanged(
        &self,
        _newdisplayname: &PCWSTR,
        _eventcontext: *const GUID,
    ) -> windows_core::Result<()> {
        debug!(self.p_id);
        Ok(())
    }

    fn OnGroupingParamChanged(
        &self,
        _newgroupingparam: *const GUID,
        _eventcontext: *const GUID,
    ) -> windows_core::Result<()> {
        debug!(self.p_id);
        Ok(())
    }

    fn OnIconPathChanged(
        &self,
        _newiconpath: &PCWSTR,
        _eventcontext: *const GUID,
    ) -> windows_core::Result<()> {
        debug!(self.p_id);
        Ok(())
    }

    fn OnSimpleVolumeChanged(
        &self,
        _newvolume: f32,
        _newmute: BOOL,
        _eventcontext: *const GUID,
    ) -> windows_core::Result<()> {
        debug!(self.p_id);
        Ok(())
    }

    fn OnChannelVolumeChanged(
        &self,
        _channelcount: u32,
        _newchannelvolumearray: *const f32,
        _changedchannel: u32,
        _eventcontext: *const GUID,
    ) -> windows_core::Result<()> {
        debug!(self.p_id);
        Ok(())
    }
}

struct SessionData {
    event: WinAudio::IAudioSessionEvents,
    control: IAudioSessionControl2
}

impl Drop for SessionData {
    fn drop(&mut self) {
        unsafe {
            let _ = self.control.UnregisterAudioSessionNotification(&self.event);
        }
    }
}