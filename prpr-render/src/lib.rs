//! prpr-render: Cross-platform rendering abstraction layer.
//!
//! Provides a unified rendering API with two backends:
//! - `wgpu`: Modern Vulkan/Metal/DX12/WebGPU backend (default)
//! - `macroquad`: Legacy OpenGL ES backend (fallback)
//!
//! The abstraction is designed around an immediate-mode 2D rendering model
//! with explicit resource management for textures, render targets, and materials.

pub mod buffer;
pub mod color;
pub mod material;
pub mod render_target;
pub mod texture;
pub mod types;

#[cfg(feature = "wgpu")]
pub mod wgpu_backend;

#[cfg(feature = "macroquad-backend")]
pub mod macroquad_backend;

pub use color::Color;
pub use types::{Mat4, Vec2, Vec3, Vec4, Vertex};

use anyhow::Result;
use async_trait::async_trait;
use std::num::NonZeroU32;

/// Render backend selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RenderBackendKind {
    /// Automatically select the best available backend.
    Auto,
    /// Modern wgpu backend (Vulkan/Metal/DX12).
    Wgpu,
    /// Legacy OpenGL ES backend via macroquad.
    OpenGl,
}

impl Default for RenderBackendKind {
    fn default() -> Self {
        Self::Auto
    }
}

/// Texture filtering mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilterMode {
    Nearest,
    Linear,
}

/// Texture wrapping mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WrapMode {
    ClampToEdge,
    Repeat,
    MirroredRepeat,
}

/// Blend state for rendering.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BlendState {
    pub enabled: bool,
    pub src_factor: BlendFactor,
    pub dst_factor: BlendFactor,
    pub equation: BlendEquation,
}

impl Default for BlendState {
    fn default() -> Self {
        Self {
            enabled: true,
            src_factor: BlendFactor::SrcAlpha,
            dst_factor: BlendFactor::OneMinusSrcAlpha,
            equation: BlendEquation::Add,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlendFactor {
    Zero,
    One,
    SrcColor,
    OneMinusSrcColor,
    SrcAlpha,
    OneMinusSrcAlpha,
    DstColor,
    OneMinusDstColor,
    DstAlpha,
    OneMinusDstAlpha,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlendEquation {
    Add,
    Subtract,
    ReverseSubtract,
    Min,
    Max,
}

/// Primitive topology for drawing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PrimitiveType {
    #[default]
    Triangles,
    TriangleStrip,
    Lines,
    LineStrip,
    Points,
}

/// Stencil operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StencilOp {
    Keep,
    Zero,
    Replace,
    IncrementClamp,
    DecrementClamp,
    Invert,
    IncrementWrap,
    DecrementWrap,
}

/// Comparison function for depth/stencil.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompareFunc {
    Never,
    Less,
    Equal,
    LessOrEqual,
    Greater,
    NotEqual,
    GreaterOrEqual,
    Always,
}

/// Stencil face state.
#[derive(Debug, Clone, Copy)]
pub struct StencilFaceState {
    pub fail_op: StencilOp,
    pub depth_fail_op: StencilOp,
    pub pass_op: StencilOp,
    pub test_func: CompareFunc,
    pub test_ref: i32,
    pub test_mask: u32,
    pub write_mask: u32,
}

impl Default for StencilFaceState {
    fn default() -> Self {
        Self {
            fail_op: StencilOp::Keep,
            depth_fail_op: StencilOp::Keep,
            pass_op: StencilOp::Keep,
            test_func: CompareFunc::Always,
            test_ref: 0,
            test_mask: u32::MAX,
            write_mask: u32::MAX,
        }
    }
}

/// Pipeline parameters for custom rendering.
#[derive(Debug, Clone, Default)]
pub struct PipelineParams {
    pub color_write: (bool, bool, bool, bool),
    pub color_blend: Option<BlendState>,
    pub stencil_test: Option<(StencilFaceState, StencilFaceState)>,
    pub primitive_type: PrimitiveType,
    pub depth_test: bool,
    pub depth_write: bool,
}

/// A handle to a GPU texture.
#[async_trait]
pub trait TextureHandle: Send + Sync {
    fn width(&self) -> u32;
    fn height(&self) -> u32;
    fn format(&self) -> TextureFormat;

    /// Update texture data from RGBA8 bytes.
    async fn set_data(&self, data: &[u8]) -> Result<()>;

    /// Generate mipmaps.
    fn generate_mipmaps(&self) -> Result<()>;

    /// Set filter mode.
    fn set_filter(&self, min: FilterMode, mag: FilterMode);

    /// Set wrap mode.
    fn set_wrap(&self, u: WrapMode, v: WrapMode);

    /// Get raw backend-specific handle (for interop).
    fn raw_handle(&self) -> u64;

    /// For downcasting to concrete backend types.
    fn as_any(&self) -> &dyn std::any::Any;
}

/// Texture pixel format.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextureFormat {
    RGBA8,
    RGB8,
    RGBA16F,
    RGBA32F,
    Depth24Plus,
    Depth32F,
}

/// A render target (framebuffer).
pub trait RenderTargetHandle: Send + Sync {
    fn width(&self) -> u32;
    fn height(&self) -> u32;
    fn texture(&self) -> &dyn TextureHandle;
    fn raw_handle(&self) -> u64;
}

/// A GPU material (shader program + uniforms).
#[async_trait]
pub trait MaterialHandle: Send + Sync {
    fn set_uniform_f32(&self, name: &str, value: f32);
    fn set_uniform_vec2(&self, name: &str, value: Vec2);
    fn set_uniform_vec3(&self, name: &str, value: Vec3);
    fn set_uniform_vec4(&self, name: &str, value: Vec4);
    fn set_uniform_mat4(&self, name: &str, value: Mat4);
    fn set_texture(&self, name: &str, texture: &dyn TextureHandle);
    fn raw_handle(&self) -> u64;
}

/// Vertex buffer for custom geometry.
pub trait VertexBufferHandle: Send + Sync {
    fn set_data(&self, vertices: &[Vertex], indices: &[u16]);
    fn raw_handle(&self) -> u64;
}

/// The main rendering backend trait.
///
/// This abstracts all GPU operations. Implementations provide
/// either a wgpu-based modern backend or a macroquad/OpenGL legacy backend.
#[async_trait]
pub trait RenderBackend: Send + Sync {
    // === Window & Surface ===

    /// Get the current window size in physical pixels.
    fn window_size(&self) -> (u32, u32);

    /// Get the current viewport (x, y, width, height) in physical pixels.
    fn viewport(&self) -> (i32, i32, i32, i32);

    /// Set the viewport.
    fn set_viewport(&self, x: i32, y: i32, w: i32, h: i32);

    // === Frame Management ===

    /// Begin a new frame. Must be called before any rendering.
    fn begin_frame(&mut self);

    /// End the current frame and present to screen.
    fn end_frame(&mut self);

    /// Clear the current render target with the given color.
    fn clear(&self, color: Color);

    // === Texture Creation ===

    /// Create a texture from RGBA8 image data.
    async fn create_texture(&self, width: u32, height: u32, data: &[u8], format: TextureFormat) -> Result<Box<dyn TextureHandle>>;

    /// Create an empty texture (for render targets).
    async fn create_render_texture(&self, width: u32, height: u32, format: TextureFormat) -> Result<Box<dyn TextureHandle>>;

    // === Render Target ===

    /// Create a render target with optional MSAA.
    async fn create_render_target(&self, width: u32, height: u32, samples: u32) -> Result<Box<dyn RenderTargetHandle>>;

    /// Set the active render target. None = screen.
    fn set_render_target(&self, target: Option<&dyn RenderTargetHandle>);

    // === Material ===

    /// Create a material from vertex and fragment shader source.
    async fn create_material(
        &self,
        vertex_shader: &str,
        fragment_shader: &str,
        uniforms: Vec<(String, UniformType)>,
        textures: Vec<String>,
    ) -> Result<Box<dyn MaterialHandle>>;

    /// Set the active material. None = default.
    fn set_material(&self, material: Option<&dyn MaterialHandle>);

    // === Vertex Buffer ===

    /// Create a vertex buffer.
    fn create_vertex_buffer(&self) -> Result<Box<dyn VertexBufferHandle>>;

    // === Immediate-Mode 2D Drawing ===

    /// Draw a filled rectangle.
    fn draw_rectangle(&self, x: f32, y: f32, w: f32, h: f32, color: Color);

    /// Draw a textured rectangle.
    fn draw_texture(&self, texture: &dyn TextureHandle, x: f32, y: f32, w: f32, h: f32, color: Color);

    /// Draw a textured rectangle with source UV rect.
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
    );

    /// Draw custom geometry.
    fn draw_geometry(&self, vertices: &[Vertex], indices: &[u16], texture: Option<&dyn TextureHandle>);

    /// Draw a vertex buffer.
    fn draw_vertex_buffer(&self, buffer: &dyn VertexBufferHandle, texture: Option<&dyn TextureHandle>);

    // === Transform & Camera ===

    /// Push a model matrix onto the stack.
    fn push_model_matrix(&self, matrix: Mat4);

    /// Pop the top model matrix.
    fn pop_model_matrix(&self);

    /// Set the projection matrix.
    fn set_projection_matrix(&self, matrix: Mat4);

    // === Pipeline State ===

    /// Set the blend state.
    fn set_blend_state(&self, state: Option<BlendState>);

    /// Set the primitive type.
    fn set_primitive_type(&self, primitive: PrimitiveType);

    /// Bind a custom pipeline (for stencil etc.).
    fn bind_pipeline(&self, params: &PipelineParams);

    // === Flush ===

    /// Flush all pending draw calls.
    fn flush(&self);

    // === Backend Info ===

    /// Get the backend kind.
    fn kind(&self) -> RenderBackendKind;

    /// Get the backend name for display.
    fn backend_name(&self) -> &str;

    /// Check if this backend supports multithreaded command recording.
    fn supports_multithreaded_rendering(&self) -> bool;
}

/// Uniform type for material parameters.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UniformType {
    Float1,
    Float2,
    Float3,
    Float4,
    Mat4,
    Int1,
    Int2,
    Int3,
    Int4,
}

/// Create a render backend instance based on the requested kind.
///
/// On failure, falls back to the legacy OpenGL backend.
pub async fn create_backend(kind: RenderBackendKind) -> Result<Box<dyn RenderBackend>> {
    match kind {
        RenderBackendKind::Wgpu => {
            #[cfg(feature = "wgpu")]
            {
                match wgpu_backend::WgpuBackend::new().await {
                    Ok(backend) => Ok(Box::new(backend)),
                    Err(e) => {
                        tracing::warn!("wgpu backend failed, falling back to OpenGL: {}", e);
                        create_opengl_backend()
                    }
                }
            }
            #[cfg(not(feature = "wgpu"))]
            {
                anyhow::bail!("wgpu feature not enabled");
            }
        }
        RenderBackendKind::OpenGl => create_opengl_backend(),
        RenderBackendKind::Auto => {
            // Try wgpu first, fall back to OpenGL
            #[cfg(feature = "wgpu")]
            {
                match wgpu_backend::WgpuBackend::new().await {
                    Ok(backend) => Ok(Box::new(backend)),
                    Err(e) => {
                        tracing::info!("wgpu not available ({}), using OpenGL fallback", e);
                        create_opengl_backend()
                    }
                }
            }
            #[cfg(not(feature = "wgpu"))]
            {
                create_opengl_backend()
            }
        }
    }
}

fn create_opengl_backend() -> Result<Box<dyn RenderBackend>> {
    #[cfg(feature = "macroquad-backend")]
    {
        Ok(Box::new(macroquad_backend::MacroquadBackend::new()?))
    }
    #[cfg(not(feature = "macroquad-backend"))]
    {
        anyhow::bail!("macroquad-backend feature not enabled, no fallback available");
    }
}

/// Safe texture wrapper that defers deletion to the render thread.
///
/// Similar to the original `SafeTexture` in prpr, but backend-agnostic.
pub struct SafeTexture {
    inner: std::sync::Arc<SafeTextureInner>,
}

struct SafeTextureInner {
    texture: Option<Box<dyn TextureHandle>>,
}

impl SafeTexture {
    pub fn new(texture: Box<dyn TextureHandle>) -> Self {
        Self {
            inner: std::sync::Arc::new(SafeTextureInner { texture: Some(texture) }),
        }
    }

    pub fn width(&self) -> u32 {
        self.inner.texture.as_ref().map(|t| t.width()).unwrap_or(0)
    }

    pub fn height(&self) -> u32 {
        self.inner.texture.as_ref().map(|t| t.height()).unwrap_or(0)
    }
}

impl Clone for SafeTexture {
    fn clone(&self) -> Self {
        Self {
            inner: std::sync::Arc::clone(&self.inner),
        }
    }
}

impl std::ops::Deref for SafeTexture {
    type Target = dyn TextureHandle;

    fn deref(&self) -> &Self::Target {
        self.inner.texture.as_ref().unwrap().as_ref()
    }
}
