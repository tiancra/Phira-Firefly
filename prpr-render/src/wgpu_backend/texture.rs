//! wgpu texture implementation.

use crate::*;
use anyhow::Result;
use async_trait::async_trait;
use std::num::NonZeroU32;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use wgpu::util::DeviceExt;

static NEXT_TEXTURE_ID: AtomicU64 = AtomicU64::new(1);

/// A GPU texture managed by wgpu.
pub struct WgpuTexture {
    pub(crate) texture: wgpu::Texture,
    pub(crate) view: wgpu::TextureView,
    pub(crate) sampler: wgpu::Sampler,
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) format: TextureFormat,
    pub(crate) bind_group_layout: wgpu::BindGroupLayout,
    pub(crate) bind_group: wgpu::BindGroup,
    pub(crate) device: Arc<wgpu::Device>,
    pub(crate) queue: Arc<wgpu::Queue>,
    pub(crate) id: u64,
}

impl WgpuTexture {
    pub fn from_data(
        device: &Arc<wgpu::Device>,
        queue: &Arc<wgpu::Queue>,
        width: u32,
        height: u32,
        data: &[u8],
        format: TextureFormat,
    ) -> Result<Self> {
        let wgpu_format = format_to_wgpu(format);
        let texture = device.create_texture_with_data(
            queue,
            &wgpu::TextureDescriptor {
                label: Some("texture"),
                size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu_format,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::RENDER_ATTACHMENT,
                view_formats: &[],
            },
            wgpu::util::TextureDataOrder::LayerMajor,
            data,
        );
        Self::from_wgpu_texture(device, queue, texture, width, height, format)
    }

    pub fn empty(
        device: &Arc<wgpu::Device>,
        queue: &Arc<wgpu::Queue>,
        width: u32,
        height: u32,
        format: TextureFormat,
    ) -> Result<Self> {
        let wgpu_format = format_to_wgpu(format);
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("render texture"),
            size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu_format,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        Self::from_wgpu_texture(device, queue, texture, width, height, format)
    }

    fn from_wgpu_texture(
        device: &Arc<wgpu::Device>,
        queue: &Arc<wgpu::Queue>,
        texture: wgpu::Texture,
        width: u32,
        height: u32,
        format: TextureFormat,
    ) -> Result<Self> {
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("texture sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("texture bind group layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        multisampled: false,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("texture bind group"),
            layout: &bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&view) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&sampler) },
            ],
        });

        Ok(Self {
            texture, view, sampler, width, height, format,
            bind_group_layout, bind_group,
            device: device.clone(),
            queue: queue.clone(),
            id: NEXT_TEXTURE_ID.fetch_add(1, Ordering::Relaxed),
        })
    }

    /// Create a 1x1 white texture for untextured rendering.
    pub fn create_white(device: &Arc<wgpu::Device>, queue: &Arc<wgpu::Queue>) -> Self {
        let data = [255u8, 255, 255, 255];
        Self::from_data(device, queue, 1, 1, &data, TextureFormat::RGBA8)
            .expect("Failed to create white texture")
    }

    pub fn wgpu_texture(&self) -> &wgpu::Texture { &self.texture }
    pub fn view(&self) -> &wgpu::TextureView { &self.view }
    pub fn sampler(&self) -> &wgpu::Sampler { &self.sampler }
    pub fn bind_group(&self) -> &wgpu::BindGroup { &self.bind_group }
    pub fn bind_group_layout(&self) -> &wgpu::BindGroupLayout { &self.bind_group_layout }
}

#[async_trait]
impl TextureHandle for WgpuTexture {
    fn width(&self) -> u32 { self.width }
    fn height(&self) -> u32 { self.height }
    fn format(&self) -> TextureFormat { self.format }

    async fn set_data(&self, data: &[u8]) -> Result<()> {
        self.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &self.texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            data,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(self.width * 4),
                rows_per_image: Some(self.height),
            },
            wgpu::Extent3d { width: self.width, height: self.height, depth_or_array_layers: 1 },
        );
        Ok(())
    }

    fn generate_mipmaps(&self) -> Result<()> { Ok(()) }
    fn set_filter(&self, min: FilterMode, mag: FilterMode) { let _ = (min, mag); }
    fn set_wrap(&self, u: WrapMode, v: WrapMode) { let _ = (u, v); }
    fn raw_handle(&self) -> u64 { self.id }
    fn as_any(&self) -> &dyn std::any::Any { self }
}

pub(crate) fn format_to_wgpu(format: TextureFormat) -> wgpu::TextureFormat {
    match format {
        TextureFormat::RGBA8 => wgpu::TextureFormat::Rgba8UnormSrgb,
        TextureFormat::RGB8 => wgpu::TextureFormat::Rgba8UnormSrgb,
        TextureFormat::RGBA16F => wgpu::TextureFormat::Rgba16Float,
        TextureFormat::RGBA32F => wgpu::TextureFormat::Rgba32Float,
        TextureFormat::Depth24Plus => wgpu::TextureFormat::Depth24Plus,
        TextureFormat::Depth32F => wgpu::TextureFormat::Depth32Float,
    }
}
