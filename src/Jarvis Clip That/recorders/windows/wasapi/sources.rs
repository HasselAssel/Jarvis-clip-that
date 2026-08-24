use anyhow::Result;

use windows::Win32::Foundation::HANDLE;

use crate::recorders::traits::Source;
use crate::recorders::windows::wasapi::env::EnvWasapi;

pub struct SourceWasapi {
    capture_client: IAudioCaptureClient,
    event: HANDLE,

    qp_frequency: i64,
    qp_start_time: i64,
}

impl SourceWasapi {
    pub fn new(env: <Self  as Source>::Env<'_>) -> Result<Self> {
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
    type Output<'o> = [u8];
    fn next_frame(
        &mut self,
        env: Self::Env<'_>
    ) -> Result<Self::Output<'_>> {

    }
}

impl Drop for SourceWasapi {
    fn drop(&mut self) {
        todo!("free event_handle")
    }
}