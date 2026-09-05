//! Immediate-mode 2D renderer for wgpu backend.
//!
//! Accumulates geometry per-texture and flushes in batches to minimize draw calls.

use crate::*;
use crate::wgpu_backend::texture::WgpuTexture;
use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Arc;

const MAX_VERTICES: usize = 4096;
const MAX_INDICES: usize = 6144;

/// A batch of geometry sharing the same texture.
struct Batch {
    vertices: Vec<Vertex>,
    indices: Vec<u16>,
    texture_id: Option<u64>,
}

/// Immediate mode renderer that accumulates geometry and flushes in batches.
pub struct ImmediateRenderer {
    device: Arc<wgpu::Device>,
    queue: Arc<wgpu::Queue>,
    batches: Vec<Batch>,
    current_texture: Option<u64>,
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    uniform_buffer: wgpu::Buffer,
    bind_group_layout: wgpu::BindGroupLayout,
    bind_group: wgpu::BindGroup,
    pipeline: wgpu::RenderPipeline,
    white_texture: Arc<WgpuTexture>,
    model_matrix: Mat4,
    projection_matrix: Mat4,
}

impl ImmediateRenderer {
    pub fn new(device: &Arc<wgpu::Device>, queue: &Arc<wgpu::Queue>) -> Self {
        let vertex_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("immediate vertex buffer"),
            size: (MAX_VERTICES * std::mem::size_of::<Vertex>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let index_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("immediate index buffer"),
            size: (MAX_INDICES * std::mem::size_of::<u16>()) as u64,
            usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("immediate uniform buffer"),
            size: 256,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("immediate bind group layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

        let white_texture = Arc::new(WgpuTexture::create_white(device, queue));

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("immediate bind group"),
            layout: &bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&white_texture.view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&white_texture.sampler),
                },
            ],
        });

        let shader_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("immediate shader"),
            source: wgpu::ShaderSource::Wgsl(IMMEDIATE_SHADER.into()),
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("immediate pipeline layout"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("immediate pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader_module,
                entry_point: Some("vs_main"),
                buffers: &[super::material::vertex_buffer_layout()],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader_module,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: wgpu::TextureFormat::Bgra8UnormSrgb,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        Self {
            device: device.clone(),
            queue: queue.clone(),
            batches: Vec::new(),
            current_texture: None,
            vertex_buffer,
            index_buffer,
            uniform_buffer,
            bind_group_layout,
            bind_group,
            pipeline,
            white_texture,
            model_matrix: Mat4::identity(),
            projection_matrix: Mat4::identity(),
        }
    }

    pub fn set_matrices(&mut self, model: Mat4, projection: Mat4) {
        self.model_matrix = model;
        self.projection_matrix = projection;
    }

    pub fn draw_rectangle(&mut self, x: f32, y: f32, w: f32, h: f32, color: Color) {
        let vertices = [
            Vertex::new(x, y, 0., 0., 0., color),
            Vertex::new(x + w, y, 0., 1., 0., color),
            Vertex::new(x, y + h, 0., 0., 1., color),
            Vertex::new(x + w, y + h, 0., 1., 1., color),
        ];
        let indices = [0u16, 1, 2, 1, 3, 2];
        self.draw_geometry(&vertices, &indices, None);
    }

    pub fn draw_texture(&mut self, texture: &dyn TextureHandle, x: f32, y: f32, w: f32, h: f32, color: Color) {
        self.draw_texture_ex(texture, x, y, w, h, 0., 0., texture.width() as f32, texture.height() as f32, color);
    }

    pub fn draw_texture_ex(
        &mut self,
        texture: &dyn TextureHandle,
        x: f32, y: f32, w: f32, h: f32,
        src_x: f32, src_y: f32, src_w: f32, src_h: f32,
        color: Color,
    ) {
        let tw = texture.width() as f32;
        let th = texture.height() as f32;
        let u0 = src_x / tw;
        let v0 = src_y / th;
        let u1 = (src_x + src_w) / tw;
        let v1 = (src_y + src_h) / th;
        let vertices = [
            Vertex::new(x, y, 0., u0, v0, color),
            Vertex::new(x + w, y, 0., u1, v0, color),
            Vertex::new(x, y + h, 0., u0, v1, color),
            Vertex::new(x + w, y + h, 0., u1, v1, color),
        ];
        let indices = [0u16, 1, 2, 1, 3, 2];
        self.draw_geometry(&vertices, &indices, Some(texture));
    }

    pub fn draw_geometry(&mut self, vertices: &[Vertex], indices: &[u16], texture: Option<&dyn TextureHandle>) {
        let tex_id = texture.map(|t| t.raw_handle());

        if self.current_texture != tex_id || self.batches.last().map_or(true, |b| b.vertices.len() + vertices.len() > MAX_VERTICES) {
            self.batches.push(Batch {
                vertices: Vec::new(),
                indices: Vec::new(),
                texture_id: tex_id,
            });
            self.current_texture = tex_id;
        }

        let batch = self.batches.last_mut().unwrap();
        let base = batch.vertices.len() as u16;
        batch.vertices.extend_from_slice(vertices);
        for &idx in indices {
            batch.indices.push(base + idx);
        }
    }

    /// Flush all accumulated batches into the given render pass.
    pub fn flush<'a>(&'a self, render_pass: &mut wgpu::RenderPass<'a>, texture_cache: &HashMap<u64, Arc<WgpuTexture>>) {
        if self.batches.is_empty() {
            return;
        }

        // Upload uniform data (model + projection matrices)
        let mut uniform_data = Vec::with_capacity(128);
        uniform_data.extend_from_slice(bytemuck::cast_slice(&self.model_matrix.cols));
        uniform_data.extend_from_slice(bytemuck::cast_slice(&self.projection_matrix.cols));
        // Pad to 256 bytes
        while uniform_data.len() < 128 {
            uniform_data.push(0);
        }
        self.queue.write_buffer(&self.uniform_buffer, 0, &uniform_data);

        render_pass.set_pipeline(&self.pipeline);
        render_pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
        render_pass.set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint16);

        let mut vertex_offset: u64 = 0;
        let mut index_offset: u64 = 0;

        for batch in &self.batches {
            if batch.vertices.is_empty() {
                continue;
            }

            // Upload vertex data
            self.queue.write_buffer(
                &self.vertex_buffer,
                vertex_offset * std::mem::size_of::<Vertex>() as u64,
                bytemuck::cast_slice(&batch.vertices),
            );

            // Upload index data
            self.queue.write_buffer(
                &self.index_buffer,
                index_offset * std::mem::size_of::<u16>() as u64,
                bytemuck::cast_slice(&batch.indices),
            );

            // Get texture view
            let texture = batch.texture_id
                .and_then(|id| texture_cache.get(&id))
                .unwrap_or(&self.white_texture);

            // Create bind group for this batch's texture
            let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("batch bind group"),
                layout: &self.bind_group_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: self.uniform_buffer.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(&texture.view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::Sampler(&texture.sampler),
                    },
                ],
            });

            render_pass.set_bind_group(0, &bind_group, &[]);
            render_pass.draw_indexed(
                0..batch.indices.len() as u32,
                (vertex_offset as i32) * std::mem::size_of::<Vertex>() as i32 / std::mem::size_of::<Vertex>() as i32,
                0..1,
            );

            vertex_offset += batch.vertices.len() as u64;
            index_offset += batch.indices.len() as u64;
        }
    }

    pub fn reset(&mut self) {
        self.batches.clear();
        self.current_texture = None;
    }
}

const IMMEDIATE_SHADER: &str = r#"
struct VertexInput {
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
    model: mat4x4<f32>,
    projection: mat4x4<f32>,
};

@group(0) @binding(0) var<uniform> uniforms: Uniforms;
@group(0) @binding(1) var tex: texture_2d<f32>;
@group(0) @binding(2) var samp: sampler;

@vertex
fn vs_main(input: VertexInput) -> VertexOutput {
    var output: VertexOutput;
    output.position = uniforms.projection * uniforms.model * vec4<f32>(input.position, 1.0);
    output.uv = input.texcoord;
    output.color = input.color0;
    return output;
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    return input.color * textureSample(tex, samp, input.uv);
}
"#;
