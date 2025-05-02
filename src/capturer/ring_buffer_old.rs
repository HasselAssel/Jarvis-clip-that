use std::{array, thread};
use std::ops::Deref;
use std::sync::{Mutex, RwLock};
use std::time::Duration;
use windows::Win32::Graphics::Direct3D11::{ID3D11Device, ID3D11DeviceContext, ID3D11Texture2D, D3D11_TEXTURE2D_DESC};

pub struct OptionalID3D11Texture2D {
    tex: ID3D11Texture2D,
    is_valid: bool,
}

impl OptionalID3D11Texture2D {
    pub fn get_tex(&self) -> &ID3D11Texture2D {
        &self.tex
    }
}

pub struct OptionalID3D11Texture2DRingBuffer<const BUFFER_LEN: usize> {
    data: [RwLock<OptionalID3D11Texture2D>; BUFFER_LEN],
    tex_desc: D3D11_TEXTURE2D_DESC,

    index: RwLock<usize>,
}

impl<const BUFFER_LEN: usize> OptionalID3D11Texture2DRingBuffer<BUFFER_LEN> {
    pub fn new(device: &ID3D11Device, tex_desc: D3D11_TEXTURE2D_DESC) -> Self {
        Self {
            data: array::from_fn(|_|
                {let mut dest_texture = None;
                    unsafe {
                        device.CreateTexture2D(&tex_desc, None, Some(&mut dest_texture)).unwrap();
                    }
                    RwLock::new(OptionalID3D11Texture2D{
                        tex: dest_texture.unwrap(),
                        is_valid: false}) }),
            tex_desc: tex_desc,
            index: RwLock::new(BUFFER_LEN)
        }
    }

    pub fn get_tex_desc(&self) -> &D3D11_TEXTURE2D_DESC {
        &self.tex_desc
    }

    pub fn copy_out<const STANDARD_OUT_LEN: usize>(&self, mutex_context: &Mutex<ID3D11DeviceContext>, buffer: &mut OptionalID3D11Texture2DRingBuffer<STANDARD_OUT_LEN>) {
        assert!(BUFFER_LEN >= STANDARD_OUT_LEN);

        let index_guard = self.index.read().unwrap();
        let start_index = ((*index_guard as i64 - STANDARD_OUT_LEN as i64) % BUFFER_LEN as i64) as usize;
        drop(index_guard);

        for i in 0..STANDARD_OUT_LEN {
            println!("Copy {}", i);
            let index = (i + start_index) % BUFFER_LEN;
            let texture_guard = self.data[index].read().unwrap();
            let mut buffer_texture_guard = buffer.data[i].write().unwrap();
            let context = mutex_context.lock().unwrap();
            unsafe {
                context.CopyResource(&buffer_texture_guard.tex, &texture_guard.tex);
            }
            drop(context);
            buffer_texture_guard.is_valid = true;

            drop(texture_guard);
            drop(buffer_texture_guard);
            thread::sleep(Duration::from_millis(2));
        }
    }

    pub fn copy_in(&self, mutex_context: &Mutex<ID3D11DeviceContext>, tex: &ID3D11Texture2D) {
        let index_guard = self.index.read().unwrap();
        let index = *index_guard;
        drop(index_guard);

        let mut texture_guard = self.data[index].write().unwrap();

        let context = mutex_context.lock().unwrap();
        unsafe {
            context.CopyResource(&texture_guard.tex, tex);
        }
        drop(context);
        texture_guard.is_valid = true;
        drop(texture_guard);
    }

    pub fn advance_index(&self) {
        let mut index_guard = self.index.write().unwrap();
        *index_guard = (*index_guard + 1) % BUFFER_LEN;
        let index = *index_guard;
        drop(index_guard);

        let mut texture_guard = self.data[index].write().unwrap();
        texture_guard.is_valid = false;
        drop(texture_guard);
    }
}

impl<const BUFFER_LEN: usize> Deref for OptionalID3D11Texture2DRingBuffer<BUFFER_LEN> {
    type Target = [RwLock<OptionalID3D11Texture2D>; BUFFER_LEN];

    fn deref(&self) -> &Self::Target {
        &self.data
    }
}