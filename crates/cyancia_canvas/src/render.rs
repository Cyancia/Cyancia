use std::{collections::HashMap, sync::Arc};

use cyancia_image::{
    layer::{Layer, LayerId},
    tile::{GpuTileStorage, GpuTileStorageInner, TileId},
};
use cyancia_math::iced_rect::{RectangleConversion, RectangleTransform};
use cyancia_render::{
    buffer::DynamicBuffer,
    resources::{FullscreenVertex, GlobalSamplers},
};
use cyancia_runtime::{
    Services,
    service::{FromRuntime, RenderContext, Service},
};
use cyancia_utils::include_shader;
use encase::ShaderType;
use glam::{Mat3, UVec2};
use iced_core::Rectangle;
use iced_widget::shader;
use parking_lot::{Mutex, RwLock};
use wgpu::{
    AddressMode, BindGroupDescriptor, BindGroupEntry, BindGroupLayout, BindGroupLayoutDescriptor,
    BindGroupLayoutEntry, BindingResource, BindingType, BlendState, BufferBindingType,
    BufferUsages, Color, ColorTargetState, ColorWrites, CommandEncoder, ComputePassDescriptor,
    ComputePipeline, ComputePipelineDescriptor, Device, Extent3d, FilterMode, FragmentState,
    LoadOp, Operations, PipelineLayoutDescriptor, Queue, RenderPassColorAttachment,
    RenderPassDescriptor, RenderPipeline, RenderPipelineDescriptor, Sampler, SamplerBindingType,
    SamplerDescriptor, ShaderModuleDescriptor, ShaderSource, ShaderStages, StorageTextureAccess,
    StoreOp, TextureDescriptor, TextureDimension, TextureFormat, TextureSampleType, TextureUsages,
    TextureView, TextureViewDescriptor, TextureViewDimension, VertexState,
    util::{BufferInitDescriptor, DeviceExt},
};

use crate::{CCanvas, CanvasId};

#[derive(Default)]
pub struct CanvasRenderers {
    instances: RwLock<HashMap<CanvasId, Arc<Mutex<CanvasRenderer>>>>,
}

impl Service for CanvasRenderers {}

impl CanvasRenderers {
    pub fn get(&self, canvas_id: &CanvasId) -> Option<Arc<Mutex<CanvasRenderer>>> {
        self.instances.read().get(canvas_id).cloned()
    }

    pub fn insert(&self, canvas_id: CanvasId, renderer: CanvasRenderer) {
        self.instances.write().insert(canvas_id, Arc::new(Mutex::new(renderer)));
    }

    pub fn remove(&self, canvas_id: &CanvasId) {
        self.instances.write().remove(canvas_id);
    }
}

#[derive(Debug)]
pub struct CanvasRenderer {
    device: Arc<Device>,
    buffer: Option<TextureView>,
    render_pipeline: CanvasRenderPipeline,
    present_pipeline: CanvasPresentPipeline,
}

impl Service for CanvasRenderer {}

impl FromRuntime for CanvasRenderer {
    fn from_runtime(runtime: &Services) -> Self {
        let render_context = runtime.service::<RenderContext>();
        let render_pipeline = CanvasRenderPipeline::new(
            &render_context.device,
            GpuTileStorageInner::TILE_FORMAT,
            runtime.service::<GlobalSamplers>().as_ref(),
        );
        let present_pipeline = CanvasPresentPipeline::new(
            &render_context.device,
            // TODO: this format varies based on platforms and hdr settings.
            TextureFormat::Bgra8Unorm,
            runtime.service::<FullscreenVertex>().as_ref(),
            runtime.service::<GlobalSamplers>().as_ref(),
        );
        Self {
            device: render_context.device.clone(),
            buffer: Default::default(),
            render_pipeline,
            present_pipeline,
        }
    }
}

impl CanvasRenderer {
    pub fn resize_buffer(&mut self, size: UVec2) {
        if let Some(buffer) = &self.buffer {
            if buffer.texture().width() == size.x && buffer.texture().height() == size.y {
                return;
            }
        }

        let texture = self.device.create_texture(&TextureDescriptor {
            label: Some("canvas render buffer"),
            size: Extent3d {
                width: size.x,
                height: size.y,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: TextureDimension::D2,
            format: GpuTileStorageInner::TILE_FORMAT,
            usage: TextureUsages::RENDER_ATTACHMENT
                | TextureUsages::STORAGE_BINDING
                | TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });

        let texture_view = texture.create_view(&TextureViewDescriptor::default());

        self.buffer = Some(texture_view);
    }

    pub fn prepare(&mut self, canvas: CanvasUniform) {
        self.render_pipeline.prepare(&self.device, canvas);
    }

    pub fn draw(
        &self,
        encoder: &mut CommandEncoder,
        clip_bounds: &Rectangle<u32>,
        target: &TextureView,
        canvas: &CCanvas,
        tile_storage: &GpuTileStorageInner,
    ) {
        let Some(buffer) = &self.buffer else {
            return;
        };

        self.render_pipeline.draw(
            &self.device,
            encoder,
            tile_storage,
            clip_bounds,
            buffer,
            canvas.image.root().id(),
        );
        self.present_pipeline
            .present(&self.device, encoder, buffer, &target, clip_bounds);
    }
}

pub struct DummyPipeline;

impl shader::Pipeline for DummyPipeline {
    fn new(device: &Device, queue: &Queue, format: TextureFormat) -> Self
    where
        Self: Sized,
    {
        Self
    }
}

pub struct CanvasPrimitive {
    pub canvas: Arc<CCanvas>,
    pub renderer: Arc<Mutex<CanvasRenderer>>,
    pub tile_storage: GpuTileStorage,
}

impl std::fmt::Debug for CanvasPrimitive {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CanvasPrimitive")
            .field("canvas", &self.canvas)
            .field("runtime", &"Runtime")
            .finish()
    }
}

impl shader::Primitive for CanvasPrimitive {
    type Pipeline = DummyPipeline;

    fn prepare(
        &self,
        renderer: &mut Self::Pipeline,
        device: &Device,
        queue: &Queue,
        bounds: &Rectangle,
        viewport: &shader::Viewport,
    ) {
        let mut renderer = self.renderer.lock();
        let size = UVec2::new(bounds.width as u32, bounds.height as u32);
        renderer.resize_buffer(size);
        let transform = self.canvas.transform.read();

        renderer.prepare(CanvasUniform {
            transform: transform.pixel_to_widget,
            inv_transform: transform.pixel_to_widget.inverse(),
            size: self.canvas.image.size(),
            total_tile_count: GpuTileStorageInner::calc_tile_count(self.canvas.image.size()),
            tile_size: GpuTileStorageInner::TILE_SIZE,
        });
    }

    fn render(
        &self,
        renderer: &Self::Pipeline,
        encoder: &mut CommandEncoder,
        target: &TextureView,
        clip_bounds: &Rectangle<u32>,
    ) {
        self.renderer.lock().draw(
            encoder,
            clip_bounds,
            target,
            &self.canvas,
            &self.tile_storage,
        );
    }
}

#[derive(Debug)]
pub struct CanvasRenderPipeline {
    pipeline: ComputePipeline,
    main_layout: BindGroupLayout,
    uniform_buffer: DynamicBuffer<CanvasUniform>,
    uniform: Option<CanvasUniform>,
    sampler: Sampler,
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
    fn new(device: &Device, format: TextureFormat, samplers: &GlobalSamplers) -> Self {
        let main_layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some("canvas main layout"),
            entries: &[
                // tiles pile
                BindGroupLayoutEntry {
                    binding: 0,
                    visibility: ShaderStages::COMPUTE,
                    ty: BindingType::Texture {
                        sample_type: TextureSampleType::Float { filterable: true },
                        view_dimension: TextureViewDimension::D2Array,
                        multisampled: false,
                    },
                    count: None,
                },
                // tile sampler
                BindGroupLayoutEntry {
                    binding: 1,
                    visibility: ShaderStages::COMPUTE,
                    ty: BindingType::Sampler(SamplerBindingType::Filtering),
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
                // tile mapper, mapping tile coords to layer indices
                BindGroupLayoutEntry {
                    binding: 3,
                    visibility: ShaderStages::COMPUTE,
                    ty: BindingType::Buffer {
                        ty: BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: Some(<u32 as ShaderType>::min_size()),
                    },
                    count: None,
                },
                // output
                BindGroupLayoutEntry {
                    binding: 4,
                    visibility: ShaderStages::COMPUTE,
                    ty: BindingType::StorageTexture {
                        access: StorageTextureAccess::WriteOnly,
                        format: format,
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

        Self {
            main_layout,
            pipeline,
            uniform_buffer: DynamicBuffer::new(
                Some("canvas uniform buffer"),
                BufferUsages::UNIFORM,
            ),
            uniform: None,
            sampler: samplers.linear_clamp().clone(),
        }
    }

    pub fn prepare(&mut self, device: &Device, uniform: CanvasUniform) {
        self.uniform_buffer.clear();
        self.uniform_buffer.push(&uniform);
        self.uniform_buffer.write_buffer(device);
        self.uniform = Some(uniform);
    }

    fn draw(
        &self,
        device: &Device,
        encoder: &mut CommandEncoder,
        tile_storage: &GpuTileStorageInner,
        clip_bounds: &Rectangle<u32>,
        target: &TextureView,
        root_layer_id: LayerId,
    ) {
        let Some(uniform) = &self.uniform else {
            return;
        };
        let Some(uniform_buffer) = self.uniform_buffer.entire_binding() else {
            return;
        };
        let target_size = target.texture().size();

        let rect_cs = clip_bounds.transform(&uniform.inv_transform);
        let visible_tiles = tile_storage.get_tile_views(
            rect_cs.as_urect(),
            uniform.total_tile_count,
            root_layer_id,
        );
        for group in visible_tiles {
            // dbg!(group.pile_texture.texture());
            let mut mapper_data =
                vec![u32::MAX; uniform.total_tile_count.element_product() as usize];
            for TileId {
                image_layer,
                index,
                pile_index,
                pile_layer,
            } in group.tiles
            {
                mapper_data
                    [index.y as usize * uniform.total_tile_count.x as usize + index.x as usize] =
                    pile_layer;
            }
            let mapper_buffer = device.create_buffer_init(&BufferInitDescriptor {
                label: Some("mapper buffer"),
                contents: bytemuck::cast_slice(&mapper_data),
                usage: BufferUsages::STORAGE,
            });

            let bind_group = device.create_bind_group(&BindGroupDescriptor {
                label: Some("canvas render bind group"),
                layout: &self.main_layout,
                entries: &[
                    BindGroupEntry {
                        binding: 0,
                        resource: BindingResource::TextureView(&group.pile),
                    },
                    BindGroupEntry {
                        binding: 1,
                        resource: BindingResource::Sampler(&self.sampler),
                    },
                    BindGroupEntry {
                        binding: 2,
                        resource: uniform_buffer.clone(),
                    },
                    BindGroupEntry {
                        binding: 3,
                        resource: mapper_buffer.as_entire_binding(),
                    },
                    BindGroupEntry {
                        binding: 4,
                        resource: BindingResource::TextureView(&target),
                    },
                ],
            });

            let mut pass = encoder.begin_compute_pass(&ComputePassDescriptor {
                label: Some("canvas render pass"),
                timestamp_writes: None,
            });

            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &bind_group, &[]);
            pass.dispatch_workgroups(
                target_size.width.div_ceil(16),
                target_size.height.div_ceil(16),
                1,
            );
        }
    }
}

#[derive(Debug, Clone)]
pub struct CanvasPresentPipeline {
    pipeline: RenderPipeline,
    layout: BindGroupLayout,
    sampler: Sampler,
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
        }
    }

    pub fn present(
        &self,
        device: &Device,
        encoder: &mut CommandEncoder,
        src: &TextureView,
        dst: &TextureView,
        clip_bounds: &Rectangle<u32>,
    ) {
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

        {
            let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
                label: Some("canvas present pass"),
                color_attachments: &[Some(RenderPassColorAttachment {
                    view: dst,
                    depth_slice: None,
                    resolve_target: None,
                    ops: Operations {
                        load: LoadOp::Clear(Color::TRANSPARENT),
                        store: StoreOp::Store,
                    },
                })],
                ..Default::default()
            });

            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &bind_group, &[]);
            pass.set_scissor_rect(
                clip_bounds.x,
                clip_bounds.y,
                clip_bounds.width,
                clip_bounds.height,
            );
            pass.draw(0..3, 0..1);
        }
    }
}
