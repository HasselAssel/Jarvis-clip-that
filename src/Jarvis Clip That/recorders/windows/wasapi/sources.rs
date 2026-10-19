use anyhow::{anyhow, Result};
use windows::Win32::Media::Audio::{IAudioCaptureClient, IAudioClient};
use windows::Win32::System::Performance::{QueryPerformanceCounter, QueryPerformanceFrequency};

use crate::debug;
use crate::recorders::traits::Source;
use crate::recorders::windows::wasapi::env::EnvWasapi;

pub struct SourceWasapi {
    capture_client: IAudioCaptureClient,

    qp_frequency: i64,
    qp_start_time: i64,
}

unsafe impl Send for SourceWasapi {} // TODO: Yea I am 67% sure this isn't safe. Change architecture to create Source on destination thread!

impl SourceWasapi {
    pub fn new(client: IAudioClient, env: &<Self  as Source>::Env<'_>) -> Result<Self> {
        let capture_client = unsafe { client.GetService()? };

        let mut qp_frequency = 0;
        let mut qp_start_time = 0;
        unsafe {
            QueryPerformanceFrequency(&mut qp_frequency)?;
            QueryPerformanceCounter(&mut qp_start_time)?;
        }
        
        Ok(Self {
            capture_client,
            qp_frequency,
            qp_start_time,
        })
    }
}

impl Source for SourceWasapi {
    type Env<'e> = EnvWasapi;
    type Output<'o> = WasApiAudioData<'o>;
    fn next_frame(
        &mut self,
        env: &Self::Env<'_>
    ) -> Result<Self::Output<'_>> {
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

        //debug!(flags);

        if packet_length == 0 {
            return Err(anyhow!("packet length is 0"));
        }

        let buffer = unsafe {
            std::slice::from_raw_parts(
                data as *const u8,
                packet_length as usize * env.format.Format.nBlockAlign as usize,
            )
        };

        Ok(WasApiAudioData {
            data: buffer,
            device_pos,
            qpc_pos,
            packet_length,
            _capture_client: &self.capture_client,
        })
    }
}

pub struct WasApiAudioData<'a> {
    pub data: &'a [u8],
    pub device_pos: u64,
    pub qpc_pos: u64,
    pub packet_length: u32,

    _capture_client: &'a IAudioCaptureClient,
}

impl<'a> Drop for WasApiAudioData<'a> {
    fn drop(&mut self) {
        unsafe { self._capture_client.ReleaseBuffer(self.packet_length).unwrap() }
    }
}

impl Drop for SourceWasapi {
    fn drop(&mut self) {
        todo!("free event_handle")
    }
}