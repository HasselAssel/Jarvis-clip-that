use anyhow::{bail, Result};
use eframe::egui;
use eframe::wgpu;

pub struct VideoTexture {
    texture: wgpu::Texture,
    queue: wgpu::Queue,
    context: egui::Context,
    pub id: egui::TextureId,
    pub width: u32,
    pub height: u32,
}

impl VideoTexture {
    pub fn new(
        render_state: &egui_wgpu::RenderState,
        queue: wgpu::Queue,
        context: egui::Context,
        width: u32,
        height: u32,
    ) -> Self {
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

        let id = render_state.renderer.write().register_native_texture(
            &render_state.device,
            &view,
            wgpu::FilterMode::Linear,
        );

        Self {
            texture,
            queue,
            context,
            id,
            width,
            height,
        }
    }

    pub fn update(
        &self,
        frame: &ffmpeg_next::frame::Video,
    ) -> Result<()> {
        if frame.width() != self.width || frame.height() != self.height || frame.format() != ffmpeg_next::format::Pixel::RGBA {
            bail!("invalid parameters ???")
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