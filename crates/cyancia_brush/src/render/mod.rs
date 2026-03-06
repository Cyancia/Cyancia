use std::{num::NonZeroU32, sync::Arc};

use cyancia_assets::{asset::AssetId, store::AssetRegistry};
use cyancia_image::{
    layer::LayerId,
    tile::{GpuTileStorage, GpuTileStorageInner, Tile},
};
use cyancia_render::buffer::{BufferVec, DynamicBuffer};
use cyancia_shader_graph::wgsl_std::nodes::{TextureId, TextureUsageRecorder};
use encase::ShaderType;
use glam::{IVec2, UVec2, Vec2};
use uuid::Uuid;
use wgpu::{
    BindGroup, BindGroupDescriptor, BindGroupEntry, BindGroupLayout, BindGroupLayoutDescriptor,
    BindGroupLayoutEntry, BindingResource, BindingType, BufferBindingType, BufferUsages,
    ComputePipeline, ComputePipelineDescriptor, Device, Extent3d, PipelineLayoutDescriptor, Queue,
    ShaderModuleDescriptor, ShaderSource, ShaderStages, StorageTextureAccess, Texture,
    TextureDescriptor, TextureDimension, TextureFormat, TextureSampleType, TextureUsages,
    TextureView, TextureViewDimension, naga::StorageAccess,
};

use crate::{
    asset::{BrushPresetInstance, GpuImage},
    render::graph::{GraphInputParams, generate_brush_shader},
};

pub mod graph;

pub const BRUSH_RENDER_TARGET: LayerId =
    LayerId::new(Uuid::from_u128(85004653408671049643065089641532));

pub struct BrushPresetOperator {
    instance: BrushPresetInstance,
    renderer: BrushPresetRenderer,
}

impl BrushPresetOperator {
    pub fn new(instance: BrushPresetInstance, device: Arc<Device>, queue: Arc<Queue>) -> Self {
        let renderer = BrushPresetRenderer::new(device, queue);
        Self { instance, renderer }
    }

    pub fn prepare(
        &mut self,
        params: GraphInputParams,
        output_layer: LayerId,
        tiles: &GpuTileStorage,
        assets: &AssetRegistry,
    ) {
        self.renderer.initialize(&mut self.instance, assets);
        self.renderer
            .prepare(&mut self.instance, params, output_layer, tiles);
    }

    pub fn draw(&self) {
        self.renderer.draw();
    }
}

#[derive(ShaderType, Debug)]
pub struct GraphInputUniform {
    pub shader_origin: IVec2,
    pub estimated_brush_size: UVec2,
    pub pen_position: Vec2,
    pub tile_size: u32,
}

#[derive(ShaderType)]
pub struct TileInfo {
    pub tile_origin: IVec2,
}

struct InitializedData {
    estimated_size: UVec2,
    output_len_in_layout: u32,
    pipeline: ComputePipeline,
    main_layout: BindGroupLayout,
}

pub struct BrushPresetRenderer {
    device: Arc<Device>,
    queue: Arc<Queue>,
    graph_input: DynamicBuffer<GraphInputUniform>,

    initialized: Option<InitializedData>,
    main_bind_group: Option<BindGroup>,
    textures: Vec<GpuImage>,
    tile_info: BufferVec<TileInfo>,

    empty_texture: GpuImage,
}

impl BrushPresetRenderer {
    pub fn new(device: Arc<Device>, queue: Arc<Queue>) -> Self {
        let empty_texture = device.create_texture(&TextureDescriptor {
            label: None,
            size: Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: TextureDimension::D2,
            format: GpuTileStorageInner::TILE_FORMAT,
            usage: TextureUsages::TEXTURE_BINDING | TextureUsages::STORAGE_BINDING,
            view_formats: &[],
        });

        Self {
            device,
            queue,
            graph_input: DynamicBuffer::default().with_usage(BufferUsages::STORAGE),

            initialized: None,
            main_bind_group: None,
            textures: Vec::new(),
            tile_info: BufferVec::default().with_usage(BufferUsages::STORAGE),
            empty_texture: GpuImage {
                texture: empty_texture,
            },
        }
    }

    pub fn initialize(&mut self, brush: &mut BrushPresetInstance, assets: &AssetRegistry) {
        let estimated_size = brush.estimate_size();
        if let Some(initialized) = self.initialized.as_ref() {
            if initialized.estimated_size == estimated_size {
                return;
            }
        }

        let estimated_tile_count = GpuTileStorageInner::calc_tile_count(brush.estimate_size()) + 2;
        let output_len = estimated_tile_count.element_product();
        // TODO: Handle shader compile error
        let (shader, texture_usage_recorder) = brush.compile().unwrap();
        println!("Generated shader:\n{}", shader);

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
                        count: Some(NonZeroU32::new(output_len).unwrap()),
                    },
                    // Textures
                    BindGroupLayoutEntry {
                        binding: 2,
                        visibility: ShaderStages::COMPUTE,
                        ty: BindingType::Texture {
                            sample_type: TextureSampleType::Float { filterable: true },
                            view_dimension: TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: Some(
                            NonZeroU32::new(texture_usage_recorder.get_usage().len() as u32)
                                .unwrap(),
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
            source: ShaderSource::Wgsl(shader.into()),
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

        self.initialized = Some(InitializedData {
            estimated_size,
            output_len_in_layout: output_len,
            pipeline,
            main_layout,
        });

        self.textures.clear();
        dbg!(&texture_usage_recorder.get_usage());
        for id in texture_usage_recorder.get_usage().keys() {
            if id == &TextureId::NULL {
                self.textures.push(self.empty_texture.clone());
                continue;
            }

            let handle = assets.handle(AssetId::new(**id)).unwrap();
            self.textures.push(GpuImage::from_asset(
                &self.device,
                &self.queue,
                &handle.get().unwrap(),
                TextureUsages::TEXTURE_BINDING,
            ));
            dbg!(id);
        }
    }

    pub fn prepare(
        &mut self,
        brush: &mut BrushPresetInstance,
        params: GraphInputParams,
        output_layer: LayerId,
        tiles: &GpuTileStorage,
    ) {
        let Some(initialized) = self.initialized.as_ref() else {
            return;
        };

        let estimated_area = brush.estimate_area(&params);
        self.graph_input.clear();
        self.graph_input.push(&GraphInputUniform {
            shader_origin: estimated_area.min,
            estimated_brush_size: initialized.estimated_size,
            tile_size: GpuTileStorageInner::TILE_SIZE,
            pen_position: params.pen_position,
        });
        self.graph_input.write_buffer(&self.device);

        let outputs = tiles.get_tiles_mut_ordered(output_layer, estimated_area);
        self.tile_info.clear();
        for tile in &outputs {
            self.tile_info.push(&TileInfo {
                tile_origin: tile.index.coord * GpuTileStorageInner::TILE_SIZE as i32,
            });
        }
        self.tile_info.write_buffer(&self.device);

        let mut empty_placeholders = Vec::new();
        let mut outputs = outputs.iter().map(|t| t.view.as_ref()).collect::<Vec<_>>();
        if outputs.len() < initialized.output_len_in_layout as usize {
            for _ in outputs.len()..initialized.output_len_in_layout as usize {
                let view = self
                    .device
                    .create_texture(&TextureDescriptor {
                        label: Some("empty placeholder texture"),
                        size: Extent3d {
                            width: 1,
                            height: 1,
                            depth_or_array_layers: 1,
                        },
                        mip_level_count: 1,
                        sample_count: 1,
                        dimension: TextureDimension::D2,
                        format: GpuTileStorageInner::TILE_FORMAT,
                        usage: TextureUsages::TEXTURE_BINDING | TextureUsages::STORAGE_BINDING,
                        view_formats: &[],
                    })
                    .create_view(&Default::default());
                empty_placeholders.push(view);
            }
            outputs.extend(&empty_placeholders);
        }

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
            layout: &initialized.main_layout,
            entries: &[
                BindGroupEntry {
                    binding: 0,
                    resource: self.graph_input.binding().unwrap(),
                },
                BindGroupEntry {
                    binding: 1,
                    resource: BindingResource::TextureViewArray(&outputs),
                },
                BindGroupEntry {
                    binding: 2,
                    resource: BindingResource::TextureViewArray(&referenced_texture_views),
                },
                BindGroupEntry {
                    binding: 3,
                    resource: self.tile_info.binding().unwrap(),
                },
            ],
        });
        self.main_bind_group = Some(main_bind_group);
    }

    pub fn draw(&self) {
        let (Some(initialized), Some(main_bind_group)) =
            (self.initialized.as_ref(), self.main_bind_group.as_ref())
        else {
            return;
        };

        let mut ec = self.device.create_command_encoder(&Default::default());

        {
            let mut pass = ec.begin_compute_pass(&Default::default());
            pass.set_pipeline(&initialized.pipeline);
            pass.set_bind_group(0, main_bind_group, &[]);
            pass.dispatch_workgroups(
                initialized.estimated_size.x.div_ceil(16),
                initialized.estimated_size.y.div_ceil(16),
                1,
            );
        }

        self.queue.submit([ec.finish()]);
    }
}
