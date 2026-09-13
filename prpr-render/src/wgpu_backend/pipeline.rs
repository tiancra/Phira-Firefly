//! Render pipeline cache for wgpu backend.

use crate::*;
use std::collections::HashMap;
use std::sync::Arc;

/// Caches render pipelines by configuration.
pub struct PipelineCache {
    pipelines: HashMap<u64, wgpu::RenderPipeline>,
    device: Arc<wgpu::Device>,
}

impl PipelineCache {
    pub fn new(device: &Arc<wgpu::Device>) -> Self {
        Self { pipelines: HashMap::new(), device: device.clone() }
    }

    pub fn get_or_create(
        &mut self,
        params: &PipelineParams,
        sample_count: u32,
        shader_module: &wgpu::ShaderModule,
    ) -> &wgpu::RenderPipeline {
        let key = self.pipeline_key(params, sample_count);
        if !self.pipelines.contains_key(&key) {
            let pipeline = self.create_pipeline(params, sample_count, shader_module);
            self.pipelines.insert(key, pipeline);
        }
        self.pipelines.get(&key).unwrap()
    }

    fn pipeline_key(&self, params: &PipelineParams, sample_count: u32) -> u64 {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let mut hasher = DefaultHasher::new();
        params.color_blend.is_some().hash(&mut hasher);
        if let Some(b) = params.color_blend {
            (b.src_factor as u8).hash(&mut hasher);
            (b.dst_factor as u8).hash(&mut hasher);
            (b.equation as u8).hash(&mut hasher);
        }
        (params.primitive_type as u8).hash(&mut hasher);
        params.depth_test.hash(&mut hasher);
        params.depth_write.hash(&mut hasher);
        params.stencil_test.is_some().hash(&mut hasher);
        sample_count.hash(&mut hasher);
        hasher.finish()
    }

    fn create_pipeline(
        &self,
        params: &PipelineParams,
        sample_count: u32,
        shader_module: &wgpu::ShaderModule,
    ) -> wgpu::RenderPipeline {
        let blend = params.color_blend.map(|bs| wgpu::BlendState {
            color: wgpu::BlendComponent {
                src_factor: convert_blend_factor(bs.src_factor),
                dst_factor: convert_blend_factor(bs.dst_factor),
                operation: convert_blend_equation(bs.equation),
            },
            alpha: wgpu::BlendComponent {
                src_factor: convert_blend_factor(bs.src_factor),
                dst_factor: convert_blend_factor(bs.dst_factor),
                operation: convert_blend_equation(bs.equation),
            },
        });

        let layout = self.device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("cached pipeline layout"),
            bind_group_layouts: &[],
            push_constant_ranges: &[],
        });

        self.device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("cached pipeline"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: shader_module,
                entry_point: Some("vs_main"),
                buffers: &[super::material::vertex_buffer_layout()],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: shader_module,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: wgpu::TextureFormat::Rgba8UnormSrgb,
                    blend,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: convert_primitive(params.primitive_type),
                ..Default::default()
            },
            depth_stencil: if params.depth_test || params.depth_write {
                Some(wgpu::DepthStencilState {
                    format: wgpu::TextureFormat::Depth24Plus,
                    depth_write_enabled: params.depth_write,
                    depth_compare: wgpu::CompareFunction::Less,
                    stencil: wgpu::StencilState::default(),
                    bias: wgpu::DepthBiasState::default(),
                })
            } else {
                None
            },
            multisample: wgpu::MultisampleState { count: sample_count, ..Default::default() },
            multiview: None,
            cache: None,
        })
    }
}

fn convert_blend_factor(f: BlendFactor) -> wgpu::BlendFactor {
    match f {
        BlendFactor::Zero => wgpu::BlendFactor::Zero,
        BlendFactor::One => wgpu::BlendFactor::One,
        BlendFactor::SrcColor => wgpu::BlendFactor::Src,
        BlendFactor::OneMinusSrcColor => wgpu::BlendFactor::OneMinusSrc,
        BlendFactor::SrcAlpha => wgpu::BlendFactor::SrcAlpha,
        BlendFactor::OneMinusSrcAlpha => wgpu::BlendFactor::OneMinusSrcAlpha,
        BlendFactor::DstColor => wgpu::BlendFactor::Dst,
        BlendFactor::OneMinusDstColor => wgpu::BlendFactor::OneMinusDst,
        BlendFactor::DstAlpha => wgpu::BlendFactor::DstAlpha,
        BlendFactor::OneMinusDstAlpha => wgpu::BlendFactor::OneMinusDstAlpha,
    }
}

fn convert_blend_equation(e: BlendEquation) -> wgpu::BlendOperation {
    match e {
        BlendEquation::Add => wgpu::BlendOperation::Add,
        BlendEquation::Subtract => wgpu::BlendOperation::Subtract,
        BlendEquation::ReverseSubtract => wgpu::BlendOperation::ReverseSubtract,
        BlendEquation::Min => wgpu::BlendOperation::Min,
        BlendEquation::Max => wgpu::BlendOperation::Max,
    }
}

fn convert_primitive(p: PrimitiveType) -> wgpu::PrimitiveTopology {
    match p {
        PrimitiveType::Triangles => wgpu::PrimitiveTopology::TriangleList,
        PrimitiveType::TriangleStrip => wgpu::PrimitiveTopology::TriangleStrip,
        PrimitiveType::Lines => wgpu::PrimitiveTopology::LineList,
        PrimitiveType::LineStrip => wgpu::PrimitiveTopology::LineStrip,
        PrimitiveType::Points => wgpu::PrimitiveTopology::PointList,
    }
}
