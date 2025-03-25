use std::array;
use std::sync::RwLock;
use windows::Win32::Graphics::Direct3D11::{ID3D11Device, ID3D11DeviceContext, ID3D11Texture2D, D3D11_TEXTURE2D_DESC};

struct OptionalID3D11Texture2D {
    tex: ID3D11Texture2D,
    is_valid: bool,
}

pub struct OptionalID3D11Texture2DRingBuffer<const LEN: usize> {
    data: [RwLock<OptionalID3D11Texture2D>; LEN],
    index: RwLock<usize>,
}

impl<const LEN: usize> OptionalID3D11Texture2DRingBuffer<LEN> {
    pub unsafe fn new(device: &ID3D11Device, tex_desc: &D3D11_TEXTURE2D_DESC) -> Self {
        Self {
            data: array::from_fn(|_|
                {let mut dest_texture = None;
                    device.CreateTexture2D(tex_desc, None, Some(&mut dest_texture)).unwrap();
                    RwLock::new(OptionalID3D11Texture2D{
                        tex: dest_texture.unwrap(),
                        is_valid: false}) }),
            index: RwLock::new(LEN)
        }
    }

    pub unsafe fn copy_out<const N_LEN: usize>(&self, buffer: &mut OptionalID3D11Texture2DRingBuffer<N_LEN>, context: &ID3D11DeviceContext) {
        if N_LEN > LEN { println!("U tryna copy elements that haven't even existed"); }

        let index_guard = self.index.read().unwrap();
        let start_index = (*index_guard + 2) % LEN;
        drop(index_guard);

        for i in start_index..(N_LEN+start_index) {
            let index = i % LEN;
            let texture_guard = self.data[index].read().unwrap();
            let mut buffer_texture_guard = buffer.data[i].write().unwrap();
            context.CopyResource(&buffer_texture_guard.tex, &texture_guard.tex);
            buffer_texture_guard.is_valid = true;

            drop(texture_guard);
            drop(buffer_texture_guard);
        }
    }

    pub unsafe fn copy_into(&mut self, context: &ID3D11DeviceContext, tex: &ID3D11Texture2D) {
        let index_guard = self.index.read().unwrap();
        let index = *index_guard;
        drop(index_guard);

        let mut texture_guard = self.data[index].write().unwrap();
        context.CopyResource(&texture_guard.tex, tex);
        texture_guard.is_valid = true;
        drop(texture_guard);
    }

    pub fn skip_current(&mut self) {
        let mut index_guard = self.index.write().unwrap();
        *index_guard = (*index_guard + 1) % LEN;
        let index = *index_guard;
        drop(index_guard);

        let mut texture_guard = self.data[index].write().unwrap();
        texture_guard.is_valid = false;
        drop(texture_guard);
    }
}