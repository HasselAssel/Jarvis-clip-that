use anyhow::{bail, Result};
use eframe::egui;
use eframe::wgpu;

pub struct VideoTextureHandle {
    pub id: egui::TextureId,
}

pub struct VideoTexture {
    texture: wgpu::Texture,
    queue: wgpu::Queue,
    context: egui::Context,
    render_state: egui_wgpu::RenderState,
    id: egui::TextureId,
    pub width: u32,
    pub height: u32,
}

impl VideoTexture {
    pub fn new(
        render_state: &egui_wgpu::RenderState,
        queue: wgpu::Queue,
        context: egui::Context,
    ) -> Self {
        let (width, height) = (1, 1);

        let (texture, view) = Self::create_texture(render_state, width, height);

        let id = render_state.renderer.write().register_native_texture(
            &render_state.device,
            &view,
            wgpu::FilterMode::Linear,
        );

        Self {
            texture,
            queue,
            context,
            render_state: render_state.clone(),
            id,
            width,
            height,
        }
    }

    pub fn handle(&self) -> VideoTextureHandle {
        VideoTextureHandle {
            id: self.id
        }
    }

    fn create_texture(
        render_state: &egui_wgpu::RenderState,
        width: u32,
        height: u32
    ) -> (wgpu::Texture, wgpu::TextureView) {
        let texture = render_state.device.create_texture(
            &wgpu::TextureDescriptor {
                label: Some("Video Texture"),
                size: wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8Unorm,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            },
        );

        let view = texture.create_view(
            &wgpu::TextureViewDescriptor::default()
        );

        (texture, view)
    }

    fn resize(&mut self, width: u32, height: u32) {
        let (new_texture, new_view) = Self::create_texture(&self.render_state, width, height);

        self.texture = new_texture;

        self.render_state.renderer.write().update_egui_texture_from_wgpu_texture(
            &self.render_state.device,
            &new_view,
            wgpu::FilterMode::Linear,
            self.id,
        );
        self.width = width;
        self.height = height;
    }

    pub fn update(
        &mut self,
        frame: &ffmpeg_next::frame::Video,
    ) -> Result<()> {
        if frame.format() != ffmpeg_next::format::Pixel::RGBA {
            bail!("invalid format ???, expect rgba")
        }

        if frame.width() != self.width || frame.height() != self.height {
            self.resize(frame.width(), frame.height());
        }

        self.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &self.texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            frame.data(0),
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(frame.stride(0) as u32),
                rows_per_image: Some(frame.height()),
            },
            wgpu::Extent3d {
                width: frame.width(),
                height: frame.height(),
                depth_or_array_layers: 1,
            },
        );

        self.context.request_repaint();

        Ok(())
    }
}