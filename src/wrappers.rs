use std::ops::Deref;

use wasapi::AudioClient;
use windows::Win32::Foundation::HANDLE;

pub struct MaybeSafeComWrapper<I: windows::core::Interface>(pub I);
unsafe impl<I: windows::core::Interface> Send for MaybeSafeComWrapper<I> {}
impl<I: windows::core::Interface> Deref for MaybeSafeComWrapper<I> {
    type Target = I;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

pub struct MaybeSafeAudioClientWrapper(AudioClient);
unsafe impl Send for MaybeSafeAudioClientWrapper {}
impl Deref for MaybeSafeAudioClientWrapper {
    type Target = AudioClient;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

pub struct MaybeSafeHANDLEWrapper(pub HANDLE);
unsafe impl Send for MaybeSafeHANDLEWrapper {}
impl Deref for MaybeSafeHANDLEWrapper {
    type Target = HANDLE;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

pub struct MaybeSafeFFIPtrWrapper<P>(pub *mut P);
unsafe impl<P> Send for MaybeSafeFFIPtrWrapper<P> {}
impl<P> Deref for MaybeSafeFFIPtrWrapper<P> {
    type Target = *mut P;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}