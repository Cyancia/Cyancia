use std::{num::NonZeroU32, sync::Arc};

use cyancia_image::tile::{GpuTileStorage, GpuTileStorageInner, Tile};
use cyancia_render::buffer::DynamicBuffer;
use encase::ShaderType;
use glam::{UVec2, Vec2};
use wgpu::{
    BindGroup, BindGroupDescriptor, BindGroupEntry, BindGroupLayout, BindGroupLayoutDescriptor,
    BindGroupLayoutEntry, BindingResource, BindingType, BufferBindingType, BufferUsages,
    ComputePipeline, ComputePipelineDescriptor, Device, PipelineLayoutDescriptor, Queue,
    ShaderModuleDescriptor, ShaderSource, ShaderStages, StorageTextureAccess, TextureFormat,
    TextureView, TextureViewDimension, naga::StorageAccess,
};

use crate::{
    asset::{BrushPresetInstance, GpuImage},
    render::graph::{GraphInputParams, generate_brush_shader},
};

pub mod graph;

#[derive(ShaderType)]
pub struct GraphInputUniform {
    pub estimated_brush_size: UVec2,
    pub pen_position: Vec2,
    pub tile_size: u32,
}

#[derive(ShaderType)]
pub struct TileInfo {
    pub tile_origin: UVec2,
}

struct PreparedRenderer {
    estimated_size: UVec2,
    pipeline: ComputePipeline,
    main_layout: BindGroupLayout,
}

pub struct BrushRenderer {
    device: Arc<Device>,
    queue: Arc<Queue>,
    graph_input: DynamicBuffer<GraphInputUniform>,

    prepared: Option<PreparedRenderer>,
    main_bind_group: Option<BindGroup>,
    textures: Vec<GpuImage>,
    tile_info: DynamicBuffer<TileInfo>,
}

impl BrushRenderer {
    pub fn new(device: Arc<Device>, queue: Arc<Queue>) -> Self {
        Self {
            device,
            queue,
            graph_input: DynamicBuffer::default().with_usage(BufferUsages::STORAGE),

            prepared: None,
            main_bind_group: None,
            textures: Vec::new(),
            tile_info: DynamicBuffer::default().with_usage(BufferUsages::STORAGE),
        }
    }

    pub fn brush_resize(&mut self, brush: &mut BrushPresetInstance) {
        let estimated_size = brush.estimate_size();
        let estimated_tile_count = GpuTileStorageInner::calc_tile_count(brush.estimate_size());

        let main_layout = self
            .device
            .create_bind_group_layout(&BindGroupLayoutDescriptor {
                label: Some("brush main layout"),
                entries: &[
                    // Graph Input Parameters
                    BindGroupLayoutEntry {
                        binding: 0,
                        visibility: ShaderStages::COMPUTE,
                        ty: BindingType::Buffer {
                            ty: BufferBindingType::Storage { read_only: true },
                            has_dynamic_offset: false,
                            min_binding_size: Some(GraphInputUniform::min_size()),
                        },
                        count: None,
                    },
                    // Output
                    BindGroupLayoutEntry {
                        binding: 1,
                        visibility: ShaderStages::COMPUTE,
                        ty: BindingType::StorageTexture {
                            access: StorageTextureAccess::WriteOnly,
                            format: TextureFormat::Rgba16Float,
                            view_dimension: TextureViewDimension::D2,
                        },
                        count: Some(
                            NonZeroU32::new(estimated_tile_count.element_product()).unwrap(),
                        ),
                    },
                    // Textures
                    BindGroupLayoutEntry {
                        binding: 2,
                        visibility: ShaderStages::COMPUTE,
                        ty: BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: Some(
                            NonZeroU32::new(brush.referenced_textures().len() as u32).unwrap(),
                        ),
                    },
                    // Tile Info
                    BindGroupLayoutEntry {
                        binding: 3,
                        visibility: ShaderStages::COMPUTE,
                        ty: BindingType::Buffer {
                            ty: BufferBindingType::Storage { read_only: true },
                            has_dynamic_offset: false,
                            min_binding_size: Some(TileInfo::min_size()),
                        },
                        count: None,
                    },
                ],
            });

        let pipeline_layout = self
            .device
            .create_pipeline_layout(&PipelineLayoutDescriptor {
                label: Some("brush pipeline layout"),
                bind_group_layouts: &[&main_layout],
                push_constant_ranges: &[],
            });

        let shader = self.device.create_shader_module(ShaderModuleDescriptor {
            label: Some("brush shader"),
            source: ShaderSource::Wgsl(
                // TODO: Handle shader compile error
                brush.compile().unwrap().into(),
            ),
        });

        let pipeline = self
            .device
            .create_compute_pipeline(&ComputePipelineDescriptor {
                label: Some("brush pipeline"),
                layout: Some(&pipeline_layout),
                module: &shader,
                entry_point: Some("main"),
                compilation_options: Default::default(),
                cache: None,
            });

        self.prepared = Some(PreparedRenderer {
            estimated_size,
            pipeline,
            main_layout,
        });
    }

    pub fn upload_textures(&mut self, brush: &mut BrushPresetInstance) {
        self.textures.clear();
        for handle in brush.referenced_textures() {
            let texture = handle.get().unwrap();
            self.textures
                .push(GpuImage::from_asset(&self.device, &self.queue, &texture));
        }
    }

    pub fn prepare(&mut self, params: GraphInputParams, outputs: &[Tile]) {
        let Some(prepared) = self.prepared.as_ref() else {
            return;
        };

        self.graph_input.clear();
        self.graph_input
            .push(&GraphInputUniform {
                estimated_brush_size: prepared.estimated_size,
                tile_size: GpuTileStorageInner::TILE_SIZE,
                pen_position: params.pen_position,
            })
            .unwrap();
        self.graph_input.write_buffer(&self.device);

        self.tile_info.clear();
        for tile in outputs {
            self.tile_info
                .push(&TileInfo {
                    tile_origin: tile.index.coord.as_uvec2() * GpuTileStorageInner::TILE_SIZE,
                })
                .unwrap();
        }
        self.tile_info.write_buffer(&self.device);

        let outputs = outputs.iter().map(|t| t.view.as_ref()).collect::<Vec<_>>();

        let referenced_textures = self
            .textures
            .iter()
            .map(|t| t.texture.create_view(&Default::default()))
            .collect::<Vec<_>>();
        let referenced_texture_views = referenced_textures
            .iter()
            .map(std::convert::identity)
            .collect::<Vec<_>>();

        let main_bind_group = self.device.create_bind_group(&BindGroupDescriptor {
            label: Some("brush main bind group"),
            layout: &prepared.main_layout,
            entries: &[
                BindGroupEntry {
                    binding: 0,
                    resource: self.graph_input.entire_binding().unwrap(),
                },
                BindGroupEntry {
                    binding: 1,
                    resource: BindingResource::TextureViewArray(&outputs),
                },
                BindGroupEntry {
                    binding: 2,
                    resource: BindingResource::TextureViewArray(&referenced_texture_views),
                },
            ],
        });
        self.main_bind_group = Some(main_bind_group);
    }

    pub fn draw(&self) {
        let (Some(prepared), Some(main_bind_group)) =
            (self.prepared.as_ref(), self.main_bind_group.as_ref())
        else {
            return;
        };

        let mut ec = self.device.create_command_encoder(&Default::default());

        {
            let mut pass = ec.begin_compute_pass(&Default::default());
            pass.set_pipeline(&prepared.pipeline);
            pass.set_bind_group(0, main_bind_group, &[]);
            pass.dispatch_workgroups(
                prepared.estimated_size.x.div_ceil(16),
                prepared.estimated_size.y.div_ceil(16),
                1,
            );
        }
    }
}
