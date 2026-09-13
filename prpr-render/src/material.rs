//! Material / shader types.

use crate::{MaterialHandle, UniformType, Vec2, Vec3, Vec4, Mat4, TextureHandle};
use anyhow::Result;
use async_trait::async_trait;
use std::collections::HashMap;

/// Null material for testing.
pub struct NullMaterial;

#[async_trait]
impl MaterialHandle for NullMaterial {
    fn set_uniform_f32(&self, _name: &str, _value: f32) {}
    fn set_uniform_vec2(&self, _name: &str, _value: Vec2) {}
    fn set_uniform_vec3(&self, _name: &str, _value: Vec3) {}
    fn set_uniform_vec4(&self, _name: &str, _value: Vec4) {}
    fn set_uniform_mat4(&self, _name: &str, _value: Mat4) {}
    fn set_texture(&self, _name: &str, _texture: &dyn TextureHandle) {}
    fn raw_handle(&self) -> u64 { 0 }
}

/// Description of a material's uniform layout.
#[derive(Debug, Clone)]
pub struct MaterialLayout {
    pub uniforms: Vec<(String, UniformType)>,
    pub textures: Vec<String>,
}

/// Shader source pair.
#[derive(Debug, Clone)]
pub struct ShaderSource {
    pub vertex: String,
    pub fragment: String,
}

/// Convert a GLSL ES 1.00 uniform type string to UniformType.
pub fn glsl_type_to_uniform(ty: &str) -> Option<UniformType> {
    match ty {
        "float" => Some(UniformType::Float1),
        "vec2" => Some(UniformType::Float2),
        "vec3" => Some(UniformType::Float3),
        "vec4" => Some(UniformType::Float4),
        "mat4" => Some(UniformType::Mat4),
        "int" => Some(UniformType::Int1),
        "ivec2" => Some(UniformType::Int2),
        "ivec3" => Some(UniformType::Int3),
        "ivec4" => Some(UniformType::Int4),
        _ => None,
    }
}
