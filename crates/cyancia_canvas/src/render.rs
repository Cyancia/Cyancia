use std::{collections::HashMap, num::NonZeroU32, sync::Arc};

use bevy_math::IRect;
use cyancia_image::{
    layer::{LayerData, LayerId},
    texel::TexelType,
    tile::{GpuTileInfo, GpuTileStorage, GpuTileStorageInner},
};
use cyancia_math::{iced_rect::IntoRect, rect_transform::RectTransform};
use cyancia_render::{
    buffer::DynamicBuffer,
    resources::{FullscreenVertex, GlobalSamplers},
};
use cyancia_runtime::{
    Services,
    service::{FromServices, RenderContext, Service},
};
use cyancia_utils::include_shader;
use encase::ShaderType;
use glam::{IVec2, Mat3, UVec2, UVec3};
use iced_core::Rectangle;
use iced_widget::shader;
use parking_lot::{Mutex, RwLock};
use wgpu::{
    AddressMode, BindGroup, BindGroupDescriptor, BindGroupEntry, BindGroupLayout,
    BindGroupLayoutDescriptor, BindGroupLayoutEntry, BindingResource, BindingType, BlendState,
    BufferBindingType, BufferUsages, Color, ColorTargetState, ColorWrites, CommandEncoder,
    ComputePassDescriptor, ComputePipeline, ComputePipelineDescriptor, Device, Extent3d,
    FilterMode, FragmentState, LoadOp, Operations, PipelineLayoutDescriptor, Queue,
    RenderPassColorAttachment, RenderPassDescriptor, RenderPipeline, RenderPipelineDescriptor,
    Sampler, SamplerBindingType, SamplerDescriptor, ShaderModuleDescriptor, ShaderSource,
    ShaderStages, StorageTextureAccess, StoreOp, TextureDescriptor, TextureDimension,
    TextureFormat, TextureSampleType, TextureUsages, TextureView, TextureViewDescriptor,
    TextureViewDimension, VertexState,
    util::{BufferInitDescriptor, DeviceExt},
};

use crate::{CCanvas, CanvasId, control::CanvasTransform};

/// When rendering canvas, we need to first compose all tiles onto a temporary surface.
/// This surface will be used as storage texture and float sampled texture.
pub const INTERMEDIATE_BUFFER_FORMAT: TextureFormat = TextureFormat::Rgba16Float;

#[derive(Debug)]
pub struct CanvasRenderer {
    buffer: Option<TextureView>,
    render_pipeline: CanvasRenderPipeline,
    present_pipeline: CanvasPresentPipeline,
}

impl Service for CanvasRenderer {}

impl CanvasRenderer {
    pub fn resize_output_buffer(&mut self, device: &Device, size: UVec2) {
        let texture = device.create_texture(&TextureDescriptor {
            label: Some("canvas render buffer"),
            size: Extent3d {
                width: size.x,
                height: size.y,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: TextureDimension::D2,
            format: INTERMEDIATE_BUFFER_FORMAT,
            usage: TextureUsages::RENDER_ATTACHMENT
                | TextureUsages::STORAGE_BINDING
                | TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });

        let texture_view = texture.create_view(&TextureViewDescriptor::default());

        self.buffer = Some(texture_view);
    }

    pub fn prepare(
        &mut self,
        device: &Device,
        queue: &Queue,
        uniform: CanvasUniform,
        tile_storage: &GpuTileStorage,
        root_layer_id: LayerId,
    ) {
        let Some(buffer) = &self.buffer else {
            return;
        };

        self.render_pipeline.prepare(
            &device,
            &queue,
            uniform,
            buffer,
            tile_storage,
            root_layer_id,
        );
        self.present_pipeline.prepare(&device, buffer);
    }

    pub fn draw(
        &self,
        encoder: &mut CommandEncoder,
        clip_bounds: &Rectangle<u32>,
        target: &TextureView,
    ) {
        let Some(buffer) = &self.buffer else {
            return;
        };

        self.render_pipeline.draw(encoder);
        self.present_pipeline.present(encoder, &target, clip_bounds);
    }
}

impl shader::Pipeline for CanvasRenderer {
    fn new(device: &Device, queue: &Queue, format: TextureFormat) -> Self
    where
        Self: Sized,
    {
        // We cannot access to services here, so just create new one.
        let fullscreen_vertex = FullscreenVertex::new(device);
        let global_samplers = GlobalSamplers::new(device);

        let render_pipeline = CanvasRenderPipeline::new();
        let present_pipeline =
            CanvasPresentPipeline::new(device, format, &fullscreen_vertex, &global_samplers);
        Self {
            buffer: Default::default(),
            render_pipeline,
            present_pipeline,
        }
    }
}

#[derive(Debug, Clone)]
pub struct CanvasPrimitive {
    pub image_size: UVec2,
    pub root_layer: LayerId,
    pub transform: CanvasTransform,
    pub tile_storage: GpuTileStorage,
}

impl shader::Primitive for CanvasPrimitive {
    type Pipeline = CanvasRenderer;

    fn prepare(
        &self,
        renderer: &mut Self::Pipeline,
        device: &Device,
        queue: &Queue,
        bounds: &Rectangle,
        viewport: &shader::Viewport,
    ) {
        let output_buffer_size = UVec2::new(bounds.width as u32, bounds.height as u32);

        if renderer.buffer.as_ref().is_none_or(|b| {
            b.texture().width() != output_buffer_size.x
                || b.texture().height() != output_buffer_size.y
        }) {
            renderer.resize_output_buffer(device, output_buffer_size);
        }

        let tile_count = GpuTileStorageInner::calc_tile_count(self.image_size);
        if renderer.render_pipeline.max_tile_count.x < tile_count.x
            || renderer.render_pipeline.max_tile_count.y < tile_count.y
        {
            renderer.render_pipeline.resize_canvas(
                device,
                self.image_size,
                self.tile_storage
                    .get_layer_info(self.root_layer)
                    .unwrap()
                    .texel_type,
            );
        }

        let uniform = CanvasUniform {
            transform: self.transform.pixel_to_widget,
            inv_transform: self.transform.pixel_to_widget.inverse(),
            size: self.image_size,
            total_tile_count: tile_count,
            tile_size: GpuTileStorageInner::TILE_SIZE,
        };

        renderer.prepare(device, queue, uniform, &self.tile_storage, self.root_layer);
    }

    fn render(
        &self,
        renderer: &Self::Pipeline,
        encoder: &mut CommandEncoder,
        target: &TextureView,
        clip_bounds: &Rectangle<u32>,
    ) {
        renderer.draw(encoder, clip_bounds, target);
    }
}

#[derive(Debug)]
pub struct CanvasRenderPipeline {
    max_tile_count: UVec2,
    pipeline: Option<ComputePipeline>,
    main_layout: Option<BindGroupLayout>,
    uniform_buffer: DynamicBuffer<CanvasUniform>,
    uniform: Option<CanvasUniform>,
    dispatch: Option<(BindGroup, UVec3)>,
}

#[derive(Debug, Clone, Copy, ShaderType)]
pub struct CanvasUniform {
    pub transform: Mat3,
    pub inv_transform: Mat3,
    pub size: UVec2,
    pub total_tile_count: UVec2,
    pub tile_size: u32,
}

impl CanvasRenderPipeline {
    fn new() -> Self {
        Self {
            max_tile_count: UVec2::ZERO,
            main_layout: None,
            pipeline: None,
            uniform_buffer: DynamicBuffer::new(
                Some("canvas uniform buffer"),
                BufferUsages::UNIFORM,
            ),
            uniform: None,
            dispatch: None,
        }
    }

    pub fn resize_canvas(&mut self, device: &Device, size: UVec2, layer_texel_type: TexelType) {
        self.max_tile_count = GpuTileStorageInner::calc_tile_count(size);
        let max_tiles = self.max_tile_count.element_product();
        let main_layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some("canvas main layout"),
            entries: &[
                // tiles
                BindGroupLayoutEntry {
                    binding: 0,
                    visibility: ShaderStages::COMPUTE,
                    ty: BindingType::StorageTexture {
                        access: StorageTextureAccess::ReadOnly,
                        format: layer_texel_type.wgpu_format(),
                        view_dimension: TextureViewDimension::D2Array,
                    },
                    count: None,
                },
                // tile info
                BindGroupLayoutEntry {
                    binding: 1,
                    visibility: ShaderStages::COMPUTE,
                    ty: BindingType::Buffer {
                        ty: BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: Some(GpuTileInfo::min_size()),
                    },
                    count: None,
                },
                // canvas uniform
                BindGroupLayoutEntry {
                    binding: 2,
                    visibility: ShaderStages::COMPUTE,
                    ty: BindingType::Buffer {
                        ty: BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: Some(<CanvasUniform as ShaderType>::min_size()),
                    },
                    count: None,
                },
                // output
                BindGroupLayoutEntry {
                    binding: 3,
                    visibility: ShaderStages::COMPUTE,
                    ty: BindingType::StorageTexture {
                        access: StorageTextureAccess::WriteOnly,
                        format: INTERMEDIATE_BUFFER_FORMAT,
                        view_dimension: TextureViewDimension::D2,
                    },
                    count: None,
                },
            ],
        });

        let pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: Some("canvas pipeline layout"),
            bind_group_layouts: &[&main_layout],
            push_constant_ranges: &[],
        });

        let shader_module = device.create_shader_module(ShaderModuleDescriptor {
            label: Some("canvas shader"),
            source: ShaderSource::Wgsl(include_shader!("canvas_render.wgsl").into()),
        });

        let pipeline = device.create_compute_pipeline(&ComputePipelineDescriptor {
            label: Some("canvas pipeline"),
            layout: Some(&pipeline_layout),
            entry_point: Some("main"),
            module: &shader_module,
            compilation_options: Default::default(),
            cache: None,
        });

        self.pipeline = Some(pipeline);
        self.main_layout = Some(main_layout);
    }

    pub fn prepare(
        &mut self,
        device: &Device,
        queue: &Queue,
        uniform: CanvasUniform,
        target: &TextureView,
        tile_storage: &GpuTileStorage,
        root_layer_id: LayerId,
    ) {
        self.uniform_buffer.clear();
        self.uniform_buffer.push(&uniform);
        self.uniform_buffer.write_buffer(device, queue);
        self.uniform = Some(uniform);

        let (Some(main_layout), Some(uniform_buffer)) =
            (&self.main_layout, self.uniform_buffer.binding())
        else {
            return;
        };
        let root_layer = tile_storage
            .get_layer_binding_or_empty(root_layer_id)
            .unwrap();
        let bind_group = device.create_bind_group(&BindGroupDescriptor {
            label: Some("canvas render bind group"),
            layout: &main_layout,
            entries: &[
                BindGroupEntry {
                    binding: 0,
                    resource: BindingResource::TextureView(&root_layer.texture),
                },
                BindGroupEntry {
                    binding: 1,
                    resource: root_layer.tile_info_buffer.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 2,
                    resource: uniform_buffer,
                },
                BindGroupEntry {
                    binding: 3,
                    resource: BindingResource::TextureView(&target),
                },
            ],
        });

        let target_size = target.texture().size();
        self.dispatch = Some((
            bind_group,
            UVec3::new(
                target_size.width.div_ceil(16),
                target_size.height.div_ceil(16),
                1,
            ),
        ));
    }

    fn draw(&self, encoder: &mut CommandEncoder) {
        let (Some(pipeline), Some((bind_group, workgroups))) = (&self.pipeline, &self.dispatch)
        else {
            return;
        };

        let mut pass = encoder.begin_compute_pass(&ComputePassDescriptor {
            label: Some("canvas render pass"),
            timestamp_writes: None,
        });

        pass.set_pipeline(&pipeline);
        pass.set_bind_group(0, bind_group, &[]);
        pass.dispatch_workgroups(workgroups.x, workgroups.y, workgroups.z);
    }
}

#[derive(Debug, Clone)]
pub struct CanvasPresentPipeline {
    pipeline: RenderPipeline,
    layout: BindGroupLayout,
    sampler: Sampler,
    bind_group: Option<BindGroup>,
}

impl CanvasPresentPipeline {
    pub fn new(
        device: &Device,
        format: TextureFormat,
        fullscreen_vertex: &FullscreenVertex,
        samplers: &GlobalSamplers,
    ) -> Self {
        let shader = device.create_shader_module(ShaderModuleDescriptor {
            label: Some("canvas present shader"),
            source: ShaderSource::Wgsl(include_shader!("canvas_present.wgsl").into()),
        });

        let layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: None,
            entries: &[
                BindGroupLayoutEntry {
                    binding: 0,
                    visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Texture {
                        sample_type: TextureSampleType::Float { filterable: true },
                        view_dimension: TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                BindGroupLayoutEntry {
                    binding: 1,
                    visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Sampler(SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

        let pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: Some("canvas present pipeline layout"),
            bind_group_layouts: &[&layout],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_render_pipeline(&RenderPipelineDescriptor {
            label: Some("canvas present pipeline"),
            layout: Some(&pipeline_layout),
            vertex: fullscreen_vertex.fullscreen_vertex_state(),
            fragment: Some(FragmentState {
                module: &shader,
                entry_point: Some("fragment"),
                targets: &[Some(ColorTargetState {
                    format,
                    blend: Some(BlendState::REPLACE),
                    write_mask: ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: Default::default(),
            depth_stencil: None,
            multisample: Default::default(),
            multiview: None,
            cache: None,
        });

        Self {
            pipeline,
            layout,
            sampler: samplers.linear_clamp().clone(),
            bind_group: None,
        }
    }

    pub fn prepare(&mut self, device: &Device, src: &TextureView) {
        let bind_group = device.create_bind_group(&BindGroupDescriptor {
            label: Some("canvas present bind group"),
            layout: &self.layout,
            entries: &[
                BindGroupEntry {
                    binding: 0,
                    resource: BindingResource::TextureView(src),
                },
                BindGroupEntry {
                    binding: 1,
                    resource: BindingResource::Sampler(&self.sampler),
                },
            ],
        });

        self.bind_group = Some(bind_group);
    }

    pub fn present(
        &self,
        encoder: &mut CommandEncoder,
        dst: &TextureView,
        clip_bounds: &Rectangle<u32>,
    ) {
        let Some(bind_group) = &self.bind_group else {
            return;
        };

        {
            let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
                label: Some("canvas present pass"),
                color_attachments: &[Some(RenderPassColorAttachment {
                    view: dst,
                    depth_slice: None,
                    resolve_target: None,
                    ops: Operations {
                        load: LoadOp::Load,
                        store: StoreOp::Store,
                    },
                })],
                ..Default::default()
            });

            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, bind_group, &[]);
            pass.set_viewport(
                clip_bounds.x as f32,
                clip_bounds.y as f32,
                clip_bounds.width as f32,
                clip_bounds.height as f32,
                0.0,
                1.0,
            );
            pass.draw(0..3, 0..1);
        }
    }
}
