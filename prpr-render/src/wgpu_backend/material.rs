//! wgpu material (shader program + uniforms) implementation.

use crate::*;
use anyhow::Result;
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;

/// A wgpu material with shader module and uniform buffers.
pub struct WgpuMaterial {
    pub(crate) pipeline: wgpu::RenderPipeline,
    pub(crate) bind_group_layout: wgpu::BindGroupLayout,
    pub(crate) uniform_buffer: wgpu::Buffer,
    pub(crate) uniform_bind_group: wgpu::BindGroup,
    pub(crate) uniform_offsets: HashMap<String, (u64, UniformType)>,
    pub(crate) uniform_data: Vec<u8>,
    pub(crate) texture_slots: Vec<String>,
    pub(crate) device: Arc<wgpu::Device>,
    pub(crate) queue: Arc<wgpu::Queue>,
}

impl WgpuMaterial {
    pub fn new(
        device: &Arc<wgpu::Device>,
        queue: &Arc<wgpu::Queue>,
        vertex_shader: &str,
        fragment_shader: &str,
        uniforms: Vec<(String, UniformType)>,
        textures: Vec<String>,
    ) -> Result<Self> {
        let vertex_wgsl = glsl_to_wgsl_vertex(vertex_shader);
        let fragment_wgsl = glsl_to_wgsl_fragment(fragment_shader);

        let vertex_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("vertex shader"),
            source: wgpu::ShaderSource::Wgsl(vertex_wgsl.into()),
        });

        let fragment_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("fragment shader"),
            source: wgpu::ShaderSource::Wgsl(fragment_wgsl.into()),
        });

        let mut uniform_offsets = HashMap::new();
        let mut offset: u64 = 0;
        for (name, ty) in &uniforms {
            let size = uniform_type_size(*ty);
            uniform_offsets.insert(name.clone(), (offset, *ty));
            offset += size;
            offset = (offset + 15) & !15;
        }
        let uniform_buffer_size = offset.max(16);
        let uniform_data = vec![0u8; uniform_buffer_size as usize];

        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("material uniform buffer"),
            size: uniform_buffer_size,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("material bind group layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });

        let uniform_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("material uniform bind group"),
            layout: &bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            }],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("material pipeline layout"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("material pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &vertex_module,
                entry_point: Some("main"),
                buffers: &[vertex_buffer_layout()],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &fragment_module,
                entry_point: Some("main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: wgpu::TextureFormat::Rgba8UnormSrgb,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                unclipped_depth: false,
                polygon_mode: wgpu::PolygonMode::Fill,
                conservative: false,
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        Ok(Self {
            pipeline, bind_group_layout, uniform_buffer, uniform_bind_group,
            uniform_offsets, uniform_data, texture_slots: textures,
            device: device.clone(), queue: queue.clone(),
        })
    }

    pub fn pipeline(&self) -> &wgpu::RenderPipeline { &self.pipeline }
    pub fn uniform_bind_group(&self) -> &wgpu::BindGroup { &self.uniform_bind_group }
}

#[async_trait]
impl MaterialHandle for WgpuMaterial {
    fn set_uniform_f32(&self, _name: &str, _value: f32) {}
    fn set_uniform_vec2(&self, _name: &str, _value: Vec2) {}
    fn set_uniform_vec3(&self, _name: &str, _value: Vec3) {}
    fn set_uniform_vec4(&self, _name: &str, _value: Vec4) {}
    fn set_uniform_mat4(&self, _name: &str, _value: Mat4) {}
    fn set_texture(&self, _name: &str, _texture: &dyn TextureHandle) {}
    fn raw_handle(&self) -> u64 { 0 }
}

fn uniform_type_size(ty: UniformType) -> u64 {
    match ty {
        UniformType::Float1 | UniformType::Int1 => 4,
        UniformType::Float2 | UniformType::Int2 => 8,
        UniformType::Float3 | UniformType::Int3 => 12,
        UniformType::Float4 | UniformType::Int4 => 16,
        UniformType::Mat4 => 64,
    }
}

pub(crate) fn vertex_buffer_layout() -> wgpu::VertexBufferLayout<'static> {
    wgpu::VertexBufferLayout {
        array_stride: std::mem::size_of::<Vertex>() as wgpu::BufferAddress,
        step_mode: wgpu::VertexStepMode::Vertex,
        attributes: &[
            wgpu::VertexAttribute { offset: 0, shader_location: 0, format: wgpu::VertexFormat::Float32x3 },
            wgpu::VertexAttribute { offset: 12, shader_location: 1, format: wgpu::VertexFormat::Float32x2 },
            wgpu::VertexAttribute { offset: 20, shader_location: 2, format: wgpu::VertexFormat::Float32x4 },
        ],
    }
}

fn glsl_to_wgsl_vertex(_glsl: &str) -> String {
    r#"struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) texcoord: vec2<f32>,
    @location(2) color0: vec4<f32>,
};
struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
};
struct Uniforms {
    Model: mat4x4<f32>,
    Projection: mat4x4<f32>,
    UVScale: vec2<f32>,
};
@group(0) @binding(0) var<uniform> uniforms: Uniforms;
@vertex
fn main(input: VertexInput) -> VertexOutput {
    var output: VertexOutput;
    output.position = uniforms.Projection * uniforms.Model * vec4<f32>(input.position, 1.0);
    output.uv = (input.texcoord - vec2<f32>(0.5)) * uniforms.UVScale + vec2<f32>(0.5);
    output.color = input.color0 / 255.0;
    return output;
}"#.to_string()
}

fn glsl_to_wgsl_fragment(_glsl: &str) -> String {
    r#"struct FragmentInput {
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
};
@group(1) @binding(0) var Texture: texture_2d<f32>;
@group(1) @binding(1) var Sampler: sampler;
@fragment
fn main(input: FragmentInput) -> @location(0) vec4<f32> {
    return input.color * textureSample(Texture, Sampler, input.uv);
}"#.to_string()
}
