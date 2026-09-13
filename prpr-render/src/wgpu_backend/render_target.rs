//! wgpu render target (framebuffer) implementation.

use crate::*;
use crate::wgpu_backend::texture::WgpuTexture;
use anyhow::Result;
use std::sync::Arc;

/// A render target with optional MSAA.
pub struct WgpuRenderTarget {
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) samples: u32,
    pub(crate) color_texture: Arc<WgpuTexture>,
    pub(crate) msaa_texture: Option<wgpu::Texture>,
    pub(crate) msaa_view: Option<wgpu::TextureView>,
    pub(crate) depth_texture: Option<wgpu::Texture>,
    pub(crate) depth_view: Option<wgpu::TextureView>,
    pub(crate) device: Arc<wgpu::Device>,
}

impl WgpuRenderTarget {
    pub fn new(device: &Arc<wgpu::Device>, queue: &Arc<wgpu::Queue>, width: u32, height: u32, samples: u32) -> Result<Self> {
        let color_texture = Arc::new(WgpuTexture::empty(device, queue, width, height, TextureFormat::RGBA8)?);

        let (msaa_texture, msaa_view) = if samples > 1 {
            let tex = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("msaa color texture"),
                size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: samples,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8UnormSrgb,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                view_formats: &[],
            });
            let view = tex.create_view(&wgpu::TextureViewDescriptor::default());
            (Some(tex), Some(view))
        } else {
            (None, None)
        };

        let (depth_texture, depth_view) = {
            let tex = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("depth texture"),
                size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: samples,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Depth24Plus,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                view_formats: &[],
            });
            let view = tex.create_view(&wgpu::TextureViewDescriptor::default());
            (Some(tex), Some(view))
        };

        Ok(Self {
            width, height, samples, color_texture,
            msaa_texture, msaa_view, depth_texture, depth_view,
            device: device.clone(),
        })
    }

    pub fn color_view(&self) -> &wgpu::TextureView { self.color_texture.view() }
    pub fn msaa_view(&self) -> Option<&wgpu::TextureView> { self.msaa_view.as_ref() }
    pub fn depth_view(&self) -> Option<&wgpu::TextureView> { self.depth_view.as_ref() }
    pub fn color_texture(&self) -> &Arc<WgpuTexture> { &self.color_texture }
}

impl RenderTargetHandle for WgpuRenderTarget {
    fn width(&self) -> u32 { self.width }
    fn height(&self) -> u32 { self.height }
    fn texture(&self) -> &dyn TextureHandle { self.color_texture.as_ref() }
    fn raw_handle(&self) -> u64 { 0 }
}
