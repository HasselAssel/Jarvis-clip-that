use anyhow::{anyhow, Result};
use windows::Win32::Foundation::HANDLE;
use windows::Win32::Media::Audio::{IAudioCaptureClient, IAudioClient};
use windows::Win32::System::Performance::{QueryPerformanceCounter, QueryPerformanceFrequency};
use windows::Win32::System::Threading::CreateEventW;

use crate::recorders::traits::Source;
use crate::recorders::windows::wasapi::env::EnvWasapi;

pub struct SourceWasapi {
    capture_client: IAudioCaptureClient,
    event: HANDLE,

    qp_frequency: i64,
    qp_start_time: i64,
}

impl SourceWasapi {
    pub fn new(client: IAudioClient, env: <Self  as Source>::Env<'_>) -> Result<Self> {
        let event;
        unsafe {
            event = CreateEventW(None, false, false, None)?;
            client.SetEventHandle(event)?;
        }
        
        let capture_client = unsafe { client.GetService()? };

        let mut qp_frequency = 0;
        let mut qp_start_time = 0;
        unsafe {
            QueryPerformanceFrequency(&mut qp_frequency)?;
            QueryPerformanceCounter(&mut qp_start_time)?;
        }
        
        Ok(Self {
            capture_client,
            event,
            qp_frequency,
            qp_start_time,
        })
    }
}

impl Source for SourceWasapi {
    type Env<'e> = EnvWasapi;
    type Output<'o> = (&'o [u8], u64);
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

        if packet_length == 0 {
            return Err(anyhow!("packet length is 0"));
        }

        let buffer = unsafe {
            std::slice::from_raw_parts(
                data as *const u8,
                packet_length as usize * env.format.Format.nBlockAlign as usize,
            )
        };

        Ok((buffer, qpc_pos))
    }
}

impl Drop for SourceWasapi {
    fn drop(&mut self) {
        todo!("free event_handle")
    }
}