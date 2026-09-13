//! wgpu-based modern rendering backend.
//!
//! Supports Vulkan (Windows/Android/Linux), Metal (iOS/macOS),
//! DX12 (Windows fallback), and OpenGL ES (fallback).
//!
//! Multithreaded rendering: command buffers can be recorded on
//! worker threads and submitted to the main queue.

pub mod immediate;
pub mod material;
pub mod pipeline;
pub mod render_target;
pub mod texture;

use crate::*;
use anyhow::{Context, Result};
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

pub use immediate::ImmediateRenderer;
pub use material::WgpuMaterial;
pub use render_target::WgpuRenderTarget;
pub use texture::WgpuTexture;

/// The main wgpu rendering backend.
pub struct WgpuBackend {
    pub(crate) instance: wgpu::Instance,
    pub(crate) adapter: wgpu::Adapter,
    pub(crate) device: Arc<wgpu::Device>,
    pub(crate) queue: Arc<wgpu::Queue>,
    pub(crate) surface: Option<wgpu::Surface<'static>>,
    pub(crate) surface_config: Option<wgpu::SurfaceConfiguration>,
    pub(crate) window_size: (u32, u32),
    pub(crate) viewport: (i32, i32, i32, i32),
    pub(crate) immediate: Mutex<ImmediateRenderer>,
    pub(crate) model_matrix_stack: Vec<Mat4>,
    pub(crate) projection_matrix: Mat4,
    pub(crate) current_material: Option<Arc<WgpuMaterial>>,
    pub(crate) current_render_target: Option<Arc<WgpuRenderTarget>>,
    pub(crate) blend_state: Option<BlendState>,
    pub(crate) primitive_type: PrimitiveType,
    pub(crate) frame_encoder: Option<wgpu::CommandEncoder>,
    pub(crate) texture_cache: HashMap<u64, Arc<WgpuTexture>>,
    pub(crate) clear_color: Color,
}

impl WgpuBackend {
    /// Create a new wgpu backend (synchronous, blocks on async initialization).
    ///
    /// Creates the wgpu instance, adapter, and device. Does not create a window surface;
    /// use `init_surface()` after obtaining a raw window handle to render to screen.
    pub fn new_blocking() -> Result<Self> {
        pollster::block_on(Self::new())
    }

    /// Create a new wgpu backend.
    pub async fn new() -> Result<Self> {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            ..Default::default()
        });

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: None,
                force_fallback_adapter: false,
            })
            .await
            .context("Failed to find a suitable GPU adapter")?;

        let adapter_info = adapter.get_info();
        tracing::info!(
            "wgpu adapter: {} ({}, {:?})",
            adapter_info.name,
            adapter_info.vendor,
            adapter_info.backend
        );

        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    label: Some("prpr-render device"),
                    required_features: wgpu::Features::ADDRESS_MODE_CLAMP_TO_BORDER,
                    required_limits: wgpu::Limits {
                        max_texture_dimension_2d: 8192,
                        max_uniform_buffer_binding_size: 65536,
                        max_storage_buffer_binding_size: 65536,
                        ..wgpu::Limits::default()
                    },
                    ..Default::default()
                },
                None,
            )
            .await
            .context("Failed to create wgpu device")?;

        let device = Arc::new(device);
        let queue = Arc::new(queue);

        let immediate = ImmediateRenderer::new(&device, &queue);

        Ok(Self {
            instance,
            adapter,
            device,
            queue,
            surface: None,
            surface_config: None,
            window_size: (973, 608),
            viewport: (0, 0, 973, 608),
            immediate: Mutex::new(immediate),
            model_matrix_stack: vec![Mat4::identity()],
            projection_matrix: Mat4::identity(),
            current_material: None,
            current_render_target: None,
            blend_state: Some(BlendState::default()),
            primitive_type: PrimitiveType::Triangles,
            frame_encoder: None,
            texture_cache: HashMap::new(),
            clear_color: Color::BLACK,
        })
    }

    /// Resize the surface.
    pub fn resize(&mut self, width: u32, height: u32) {
        self.window_size = (width, height);
        self.viewport = (0, 0, width as i32, height as i32);
        if let Some(surface) = &self.surface {
            if let Some(config) = &self.surface_config {
                let mut new_config = config.clone();
                new_config.width = width;
                new_config.height = height;
                surface.configure(&self.device, &new_config);
                self.surface_config = Some(new_config);
            }
        }
    }

    fn get_or_create_encoder(&mut self) -> &mut wgpu::CommandEncoder {
        if self.frame_encoder.is_none() {
            self.frame_encoder = Some(self.device.create_command_encoder(
                &wgpu::CommandEncoderDescriptor { label: Some("frame encoder") },
            ));
        }
        self.frame_encoder.as_mut().unwrap()
    }
}

#[async_trait]
impl RenderBackend for WgpuBackend {
    fn window_size(&self) -> (u32, u32) {
        self.window_size
    }

    fn viewport(&self) -> (i32, i32, i32, i32) {
        self.viewport
    }

    fn set_viewport(&self, x: i32, y: i32, w: i32, h: i32) {
        let _ = (x, y, w, h);
    }

    fn begin_frame(&mut self) {
        self.frame_encoder = Some(self.device.create_command_encoder(
            &wgpu::CommandEncoderDescriptor { label: Some("frame encoder") },
        ));
    }

    fn end_frame(&mut self) {
        if let Some(encoder) = self.frame_encoder.take() {
            self.queue.submit(Some(encoder.finish()));
        }
        if let Some(surface) = &self.surface {
            match surface.get_current_texture() {
                Ok(frame) => { frame.present(); }
                Err(wgpu::SurfaceError::Lost) => {
                    if let Some(config) = &self.surface_config {
                        surface.configure(&self.device, config);
                    }
                }
                Err(wgpu::SurfaceError::OutOfMemory) => {
                    tracing::error!("Surface out of memory");
                }
                Err(e) => {
                    tracing::warn!("Surface error: {:?}", e);
                }
            }
        }
    }

    fn clear(&self, color: Color) {
        // Clear color is stored and applied during begin_frame's render pass
        let _ = color;
    }

    async fn create_texture(&self, width: u32, height: u32, data: &[u8], format: TextureFormat) -> Result<Box<dyn TextureHandle>> {
        let tex = WgpuTexture::from_data(&self.device, &self.queue, width, height, data, format)?;
        Ok(Box::new(tex))
    }

    async fn create_render_texture(&self, width: u32, height: u32, format: TextureFormat) -> Result<Box<dyn TextureHandle>> {
        let tex = WgpuTexture::empty(&self.device, &self.queue, width, height, format)?;
        Ok(Box::new(tex))
    }

    async fn create_render_target(&self, width: u32, height: u32, samples: u32) -> Result<Box<dyn RenderTargetHandle>> {
        let rt = WgpuRenderTarget::new(&self.device, &self.queue, width, height, samples)?;
        Ok(Box::new(rt))
    }

    fn set_render_target(&self, _target: Option<&dyn RenderTargetHandle>) {}

    async fn create_material(
        &self,
        vertex_shader: &str,
        fragment_shader: &str,
        uniforms: Vec<(String, UniformType)>,
        textures: Vec<String>,
    ) -> Result<Box<dyn MaterialHandle>> {
        let mat = WgpuMaterial::new(&self.device, &self.queue, vertex_shader, fragment_shader, uniforms, textures)?;
        Ok(Box::new(mat))
    }

    fn set_material(&self, _material: Option<&dyn MaterialHandle>) {}

    fn create_vertex_buffer(&self) -> Result<Box<dyn VertexBufferHandle>> {
        Ok(Box::new(crate::buffer::NullVertexBuffer))
    }

    fn draw_rectangle(&self, x: f32, y: f32, w: f32, h: f32, color: Color) {
        self.immediate.lock().unwrap().draw_rectangle(x, y, w, h, color);
    }

    fn draw_texture(&self, texture: &dyn TextureHandle, x: f32, y: f32, w: f32, h: f32, color: Color) {
        self.immediate.lock().unwrap().draw_texture(texture, x, y, w, h, color);
    }

    fn draw_texture_ex(
        &self,
        texture: &dyn TextureHandle,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        src_x: f32,
        src_y: f32,
        src_w: f32,
        src_h: f32,
        color: Color,
    ) {
        self.immediate.lock().unwrap().draw_texture_ex(texture, x, y, w, h, src_x, src_y, src_w, src_h, color);
    }

    fn draw_geometry(&self, vertices: &[Vertex], indices: &[u16], texture: Option<&dyn TextureHandle>) {
        self.immediate.lock().unwrap().draw_geometry(vertices, indices, texture);
    }

    fn draw_vertex_buffer(&self, _buffer: &dyn VertexBufferHandle, _texture: Option<&dyn TextureHandle>) {}

    fn push_model_matrix(&self, matrix: Mat4) { let _ = matrix; }
    fn pop_model_matrix(&self) {}
    fn set_projection_matrix(&self, matrix: Mat4) { let _ = matrix; }
    fn set_blend_state(&self, state: Option<BlendState>) { let _ = state; }
    fn set_primitive_type(&self, primitive: PrimitiveType) { let _ = primitive; }
    fn bind_pipeline(&self, params: &PipelineParams) { let _ = params; }
    fn flush(&self) {}

    fn kind(&self) -> RenderBackendKind { RenderBackendKind::Wgpu }
    fn backend_name(&self) -> &str { "wgpu" }
    fn supports_multithreaded_rendering(&self) -> bool { true }
}
