pub struct UnsafeComWrapper<O: windows::core::Interface>(pub O);

unsafe impl<O: windows::core::Interface> Send for UnsafeComWrapper<O> {}

unsafe impl<O: windows::core::Interface> Sync for UnsafeComWrapper<O> {}

impl<O: windows::core::Interface> std::ops::Deref for UnsafeComWrapper<O> {
    type Target = O;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}