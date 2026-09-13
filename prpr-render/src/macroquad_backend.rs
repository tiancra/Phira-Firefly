//! Legacy OpenGL ES backend via macroquad/miniquad.
//!
//! Used as a fallback when wgpu is unavailable or when the user
//! explicitly selects the OpenGL backend in settings.

use crate::*;
use anyhow::Result;
use async_trait::async_trait;

/// Macroquad-based OpenGL backend.
///
/// This wraps the existing macroquad API to implement the RenderBackend trait,
/// allowing the rest of the codebase to use the unified rendering API while
/// falling back to the proven OpenGL path.
pub struct MacroquadBackend {
    window_size: (u32, u32),
    viewport: (i32, i32, i32, i32),
}

impl MacroquadBackend {
    pub fn new() -> Result<Self> {
        Ok(Self {
            window_size: (973, 608),
            viewport: (0, 0, 973, 608),
        })
    }
}

#[async_trait]
impl RenderBackend for MacroquadBackend {
    fn window_size(&self) -> (u32, u32) {
        self.window_size
    }

    fn viewport(&self) -> (i32, i32, i32, i32) {
        self.viewport
    }

    fn set_viewport(&self, x: i32, y: i32, w: i32, h: i32) {
        let _ = (x, y, w, h);
        // macroquad handles viewport via camera
    }

    fn begin_frame(&mut self) {
        // macroquad handles frame lifecycle
    }

    fn end_frame(&mut self) {
        // macroquad handles frame lifecycle
    }

    fn clear(&self, color: Color) {
        macroquad::window::clear_background(macroquad::color::Color::new(color.r, color.g, color.b, color.a));
    }

    async fn create_texture(&self, width: u32, height: u32, data: &[u8], format: TextureFormat) -> Result<Box<dyn TextureHandle>> {
        let _ = format;
        let tex = macroquad::texture::Texture2D::from_rgba8(width as _, height as _, data);
        Ok(Box::new(MacroquadTexture(tex)))
    }

    async fn create_render_texture(&self, width: u32, height: u32, format: TextureFormat) -> Result<Box<dyn TextureHandle>> {
        let _ = format;
        let tex = macroquad::texture::Texture2D::empty();
        // TODO: create render texture properly
        Ok(Box::new(MacroquadTexture(tex)))
    }

    async fn create_render_target(&self, width: u32, height: u32, samples: u32) -> Result<Box<dyn RenderTargetHandle>> {
        Ok(Box::new(MacroquadRenderTarget { width, height, samples }))
    }

    fn set_render_target(&self, target: Option<&dyn RenderTargetHandle>) {
        let _ = target;
    }

    async fn create_material(
        &self,
        vertex_shader: &str,
        fragment_shader: &str,
        uniforms: Vec<(String, UniformType)>,
        textures: Vec<String>,
    ) -> Result<Box<dyn MaterialHandle>> {
        let params = macroquad::material::MaterialParams {
            uniforms: uniforms.into_iter().map(|(n, t)| (n, convert_uniform_type(t))).collect(),
            textures,
            ..Default::default()
        };
        let material = macroquad::material::load_material(vertex_shader, fragment_shader, params)?;
        Ok(Box::new(MacroquadMaterial(material)))
    }

    fn set_material(&self, material: Option<&dyn MaterialHandle>) {
        if let Some(mat) = material {
            if let Some(mq_mat) = mat.as_any().downcast_ref::<MacroquadMaterial>() {
                macroquad::material::gl_use_material(mq_mat.0);
            }
        } else {
            macroquad::material::gl_use_default_material();
        }
    }

    fn create_vertex_buffer(&self) -> Result<Box<dyn VertexBufferHandle>> {
        Ok(Box::new(crate::buffer::NullVertexBuffer))
    }

    fn draw_rectangle(&self, x: f32, y: f32, w: f32, h: f32, color: Color) {
        macroquad::shapes::draw_rectangle(x, y, w, h, macroquad::color::Color::new(color.r, color.g, color.b, color.a));
    }

    fn draw_texture(&self, texture: &dyn TextureHandle, x: f32, y: f32, w: f32, h: f32, color: Color) {
        if let Some(tex) = texture.as_any().downcast_ref::<MacroquadTexture>() {
            macroquad::texture::draw_texture_ex(
                tex.0,
                x,
                y,
                macroquad::color::Color::new(color.r, color.g, color.b, color.a),
                macroquad::texture::DrawTextureParams {
                    dest_size: Some(macroquad::math::vec2(w, h)),
                    ..Default::default()
                },
            );
        }
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
        if let Some(tex) = texture.as_any().downcast_ref::<MacroquadTexture>() {
            macroquad::texture::draw_texture_ex(
                tex.0,
                x,
                y,
                macroquad::color::Color::new(color.r, color.g, color.b, color.a),
                macroquad::texture::DrawTextureParams {
                    source: Some(macroquad::math::Rect::new(src_x, src_y, src_w, src_h)),
                    dest_size: Some(macroquad::math::vec2(w, h)),
                    ..Default::default()
                },
            );
        }
    }

    fn draw_geometry(&self, vertices: &[Vertex], indices: &[u16], texture: Option<&dyn TextureHandle>) {
        let gl = unsafe { macroquad::window::get_internal_gl() };
        let mq_vertices: Vec<macroquad::models::Vertex> = vertices
            .iter()
            .map(|v| macroquad::models::Vertex::new(v.position[0], v.position[1], v.position[2], v.uv[0], v.uv[1], macroquad::color::Color::new(v.color[0], v.color[1], v.color[2], v.color[3])))
            .collect();
        gl.quad_gl.draw_mode(macroquad::models::DrawMode::Triangles);
        if let Some(tex) = texture.and_then(|t| t.as_any().downcast_ref::<MacroquadTexture>()) {
            gl.quad_gl.texture(Some(tex.0));
        } else {
            gl.quad_gl.texture(None);
        }
        gl.quad_gl.geometry(&mq_vertices, indices);
    }

    fn draw_vertex_buffer(&self, _buffer: &dyn VertexBufferHandle, _texture: Option<&dyn TextureHandle>) {}

    fn push_model_matrix(&self, matrix: Mat4) {
        let gl = unsafe { macroquad::window::get_internal_gl() };
        gl.quad_gl.push_model_matrix(macroquad::math::Mat4::from_cols_array(&matrix.cols));
    }

    fn pop_model_matrix(&self) {
        let gl = unsafe { macroquad::window::get_internal_gl() };
        gl.quad_gl.pop_model_matrix();
    }

    fn set_projection_matrix(&self, matrix: Mat4) {
        let _ = matrix;
    }

    fn set_blend_state(&self, state: Option<BlendState>) {
        let _ = state;
    }

    fn set_primitive_type(&self, primitive: PrimitiveType) {
        let gl = unsafe { macroquad::window::get_internal_gl() };
        gl.quad_gl.draw_mode(match primitive {
            PrimitiveType::Triangles => macroquad::models::DrawMode::Triangles,
            PrimitiveType::TriangleStrip => macroquad::models::DrawMode::TriangleStrip,
            PrimitiveType::Lines => macroquad::models::DrawMode::Lines,
            PrimitiveType::LineStrip => macroquad::models::DrawMode::LineStrip,
            PrimitiveType::Points => macroquad::models::DrawMode::Points,
        });
    }

    fn bind_pipeline(&self, params: &PipelineParams) {
        let _ = params;
    }

    fn flush(&self) {
        let mut gl = unsafe { macroquad::window::get_internal_gl() };
        gl.flush();
    }

    fn kind(&self) -> RenderBackendKind {
        RenderBackendKind::OpenGl
    }

    fn backend_name(&self) -> &str {
        "OpenGL (macroquad)"
    }

    fn supports_multithreaded_rendering(&self) -> bool {
        false
    }
}

fn convert_uniform_type(ty: UniformType) -> miniquad::UniformType {
    match ty {
        UniformType::Float1 => miniquad::UniformType::Float1,
        UniformType::Float2 => miniquad::UniformType::Float2,
        UniformType::Float3 => miniquad::UniformType::Float3,
        UniformType::Float4 => miniquad::UniformType::Float4,
        UniformType::Mat4 => miniquad::UniformType::Mat4,
        UniformType::Int1 => miniquad::UniformType::Int1,
        UniformType::Int2 => miniquad::UniformType::Int2,
        UniformType::Int3 => miniquad::UniformType::Int3,
        UniformType::Int4 => miniquad::UniformType::Int4,
    }
}

/// Macroquad texture wrapper.
pub struct MacroquadTexture(pub macroquad::texture::Texture2D);

#[async_trait]
impl TextureHandle for MacroquadTexture {
    fn width(&self) -> u32 { self.0.width() as u32 }
    fn height(&self) -> u32 { self.0.height() as u32 }
    fn format(&self) -> TextureFormat { TextureFormat::RGBA8 }
    async fn set_data(&self, _data: &[u8]) -> Result<()> { Ok(()) }
    fn generate_mipmaps(&self) -> Result<()> { Ok(()) }
    fn set_filter(&self, _min: FilterMode, _mag: FilterMode) {}
    fn set_wrap(&self, _u: WrapMode, _v: WrapMode) {}
    fn raw_handle(&self) -> u64 { self.0.raw_miniquad_texture_handle().gl_internal_id() as u64 }
    fn as_any(&self) -> &dyn std::any::Any { self }
}

/// Macroquad render target wrapper.
pub struct MacroquadRenderTarget {
    width: u32,
    height: u32,
    samples: u32,
}

impl RenderTargetHandle for MacroquadRenderTarget {
    fn width(&self) -> u32 { self.width }
    fn height(&self) -> u32 { self.height }
    fn texture(&self) -> &dyn TextureHandle {
        static DUMMY: crate::texture::NullTexture = crate::texture::NullTexture::new(1, 1, TextureFormat::RGBA8);
        &DUMMY
    }
    fn raw_handle(&self) -> u64 { 0 }
}

/// Macroquad material wrapper.
pub struct MacroquadMaterial(pub macroquad::material::Material);

#[async_trait]
impl MaterialHandle for MacroquadMaterial {
    fn set_uniform_f32(&self, name: &str, value: f32) { self.0.set_uniform(name, value); }
    fn set_uniform_vec2(&self, name: &str, value: Vec2) { self.0.set_uniform(name, macroquad::math::vec2(value.x, value.y)); }
    fn set_uniform_vec3(&self, name: &str, value: Vec3) { self.0.set_uniform(name, macroquad::math::vec3(value.x, value.y, value.z)); }
    fn set_uniform_vec4(&self, name: &str, value: Vec4) { self.0.set_uniform(name, macroquad::color::Color::new(value.x, value.y, value.z, value.w)); }
    fn set_uniform_mat4(&self, name: &str, value: Mat4) { self.0.set_uniform(name, macroquad::math::Mat4::from_cols_array(&value.cols)); }
    fn set_texture(&self, name: &str, texture: &dyn TextureHandle) {
        if let Some(tex) = texture.as_any().downcast_ref::<MacroquadTexture>() {
            self.0.set_texture(name, tex.0);
        }
    }
    fn raw_handle(&self) -> u64 { 0 }
}
