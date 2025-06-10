pub struct ComObj<O: windows::core::Interface>(pub O);

unsafe impl<O: windows::core::Interface> Send for ComObj<O> {}

unsafe impl<O: windows::core::Interface> Sync for ComObj<O> {}

impl<O: windows::core::Interface> std::ops::Deref for ComObj<O> {
    type Target = O;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

//TODO:  other things like hw_frame_ctx and event_handle also need to be sent between threads!