use windows::Win32::System::Performance::QueryPerformanceCounter;
use windows::Win32::System::Performance::QueryPerformanceFrequency;

#[derive(Clone)]
pub struct Moment {
    pub pos: i64,
    pub time_base: (i64, i64),
}

pub fn now() -> Option<Moment> {
    #[cfg(target_os = "windows")]
    {
        let mut qpc = 0;
        let mut freq = 0;
        unsafe {
            QueryPerformanceCounter(&mut qpc).ok()?;
            QueryPerformanceFrequency(&mut freq).ok()?;
        }
        Some(Moment {
            pos: qpc,
            time_base: (1, freq),
        })
    }

    #[cfg(target_os = "linux")]
    todo!()
}