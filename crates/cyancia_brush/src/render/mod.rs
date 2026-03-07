use std::{num::NonZeroU32, sync::Arc};

use bevy_math::IRect;
use cyancia_assets::{asset::AssetId, store::AssetRegistry};
use cyancia_image::{
    layer::LayerId,
    texel::{TexelDepth, TexelFormat, TexelType},
    tile::{GpuTileStorage, GpuTileStorageInner, Tile},
};
use cyancia_render::buffer::{BufferVec, DynamicBuffer};
use cyancia_shader_graph::{
    graph::node::external::generate_external_variable_binding,
    wgsl_std::nodes::{TextureId, TextureUsageRecorder},
};
use encase::ShaderType;
use glam::{IVec2, UVec2, Vec2};
use uuid::Uuid;
use wesl::{VirtualResolver, Wesl};
use wgpu::{
    BindGroup, BindGroupDescriptor, BindGroupEntry, BindGroupLayout, BindGroupLayoutDescriptor,
    BindGroupLayoutEntry, BindingResource, BindingType, Buffer, BufferBindingType, BufferUsages,
    ComputePipeline, ComputePipelineDescriptor, Device, Extent3d, PipelineLayoutDescriptor, Queue,
    ShaderModuleDescriptor, ShaderSource, ShaderStages, StorageTextureAccess, Texture,
    TextureDescriptor, TextureDimension, TextureFormat, TextureSampleType, TextureUsages,
    TextureView, TextureViewDimension,
    naga::StorageAccess,
    util::{BufferInitDescriptor, DeviceExt},
};

use crate::{
    asset::{BrushPresetInstance, CompiledBrushPreset, GpuImage},
    render::graph::GraphInputParams,
};

pub mod graph;

// Ping pong buffer
pub const STROKE_INTERMEDIATE_SURFACE_A: LayerId =
    LayerId::new(Uuid::from_u128(74806453124856123074153214863542));
pub const STROKE_INTERMEDIATE_SURFACE_B: LayerId =
    LayerId::new(Uuid::from_u128(74806453124856123074153214863543));
pub const STROKE_INTERMEDIATE_SURFACES: [LayerId; 2] =
    [STROKE_INTERMEDIATE_SURFACE_A, STROKE_INTERMEDIATE_SURFACE_B];

const EXTERNAL_VARIABLE_BASE_BINDING: u32 = 6;

pub struct BrushPresetOperator {
    instance: Arc<BrushPresetInstance>,
    renderer: BrushPresetRenderer,
}

impl BrushPresetOperator {
    pub fn new(instance: Arc<BrushPresetInstance>, device: Arc<Device>, queue: Arc<Queue>) -> Self {
        let renderer = BrushPresetRenderer::new(device, queue);
        Self { instance, renderer }
    }

    pub fn begin_stroke(&mut self, tiles: &GpuTileStorage, assets: &AssetRegistry) {
        tiles.declare_layer(
            STROKE_INTERMEDIATE_SURFACE_A,
            TexelType {
                format: TexelFormat::Rgba,
                depth: TexelDepth::Bit8,
            },
        );
        tiles.declare_layer(
            STROKE_INTERMEDIATE_SURFACE_B,
            TexelType {
                format: TexelFormat::Rgba,
                depth: TexelDepth::Bit8,
            },
        );
        tiles.clear_layer(STROKE_INTERMEDIATE_SURFACE_A);
        tiles.clear_layer(STROKE_INTERMEDIATE_SURFACE_B);

        let target_layer_texel = tiles
            .layer_texel_type(STROKE_INTERMEDIATE_SURFACE_A)
            .unwrap();
        if self
            .renderer
            .initialized
            .as_ref()
            .is_none_or(|initialized| {
                let estimated_size = self.instance.estimate_size();
                initialized.estimated_size != estimated_size
                    || initialized.target_layer_texel != target_layer_texel
            })
        {
            self.renderer
                .initialize(&mut self.instance, assets, target_layer_texel);
        }

        if let Some(initialized) = self.renderer.initialized.as_mut() {
            initialized.accumulated_area = IRect::EMPTY;
        }
        self.renderer.main_prepared = None;
        self.renderer.stroke_postprocess_prepared = None;
    }

    pub fn update_stroke(
        &mut self,
        params: GraphInputParams,
        target_layer: LayerId,
        tiles: &GpuTileStorage,
    ) {
        self.renderer
            .prepare_main(&mut self.instance, params, target_layer, tiles);
        self.renderer.draw();
        // self.renderer.prepare_stroke_postprocess(tiles);
        // self.renderer.merge_down();
    }

    pub fn end_stroke(&mut self, tiles: &GpuTileStorage, target_layer: LayerId) {
        self.renderer
            .prepare_stroke_postprocess(tiles, target_layer);
        self.renderer.merge_down();
    }

    pub fn draw(&self) {}
}

#[derive(ShaderType, Debug)]
pub struct GraphInputUniform {
    pub pen_position: Vec2,
}

#[derive(ShaderType, Debug)]
pub struct StrokeInfoUniform {
    pub shader_origin: IVec2,
    pub estimated_brush_size: UVec2,
    pub tile_size: u32,
}

#[derive(ShaderType)]
pub struct TileInfo {
    pub tile_origin: IVec2,
}

struct InitializedData {
    estimated_size: UVec2,
    output_len_in_layout: u32,
    main_pipeline: ComputePipeline,
    main_layout: BindGroupLayout,
    target_layer_texel: TexelType,
    textures: Vec<TextureView>,
    accumulated_area: IRect,
    compiled_stroke_postprocess_graphs: Vec<String>,
    external_var_bind_layout_entries: Vec<BindGroupLayoutEntry>,
    main_graph_input: DynamicBuffer<GraphInputUniform>,
    stroke_info: DynamicBuffer<StrokeInfoUniform>,
}

struct MainPreparedData {
    main_bind_group: BindGroup,
    external_var_buffers: Vec<Buffer>,
    estimated_area: IRect,
}

struct StrokePostprocessPreparedData {
    pipelines: Vec<ComputePipeline>,
    bind_groups: Vec<BindGroup>,
}

pub struct BrushPresetRenderer {
    device: Arc<Device>,
    queue: Arc<Queue>,

    initialized: Option<InitializedData>,
    main_prepared: Option<MainPreparedData>,
    stroke_postprocess_prepared: Option<StrokePostprocessPreparedData>,

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
            // Random texture that can be binded as texture_2d<f32>
            format: TextureFormat::Rgba8Unorm,
            usage: TextureUsages::TEXTURE_BINDING | TextureUsages::STORAGE_BINDING,
            view_formats: &[],
        });

        Self {
            device,
            queue,
            empty_texture: GpuImage {
                texture: empty_texture,
            },

            initialized: None,
            main_prepared: None,
            stroke_postprocess_prepared: None,
        }
    }

    pub fn initialize(
        &mut self,
        brush: &BrushPresetInstance,
        assets: &AssetRegistry,
        target_layer_texel: TexelType,
    ) {
        let estimated_size = brush.estimate_size();

        let estimated_tile_count = GpuTileStorageInner::calc_tile_count(brush.estimate_size()) + 2;
        let output_len = estimated_tile_count.element_product();
        // TODO: Handle shader compile error
        let compiled_preset = brush.compile(EXTERNAL_VARIABLE_BASE_BINDING).unwrap();
        println!("Compiled brush preset: \n{compiled_preset}");

        // Prepare referenced textures

        let mut textures = Vec::new();
        for id in compiled_preset.texture_usages {
            if id == TextureId::NULL {
                textures.push(self.empty_texture.texture.create_view(&Default::default()));
                continue;
            }

            let handle = assets.handle(AssetId::new(*id)).unwrap();
            let gpu_image = GpuImage::from_asset(
                &self.device,
                &self.queue,
                &handle.get().unwrap(),
                TextureUsages::TEXTURE_BINDING,
            );
            textures.push(gpu_image.texture.create_view(&Default::default()));
        }

        // Prepare bind group layout

        let mut main_layout_entries = vec![
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
            // Stroke Info
            BindGroupLayoutEntry {
                binding: 1,
                visibility: ShaderStages::COMPUTE,
                ty: BindingType::Buffer {
                    ty: BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: Some(StrokeInfoUniform::min_size()),
                },
                count: None,
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
                count: Some(NonZeroU32::new(textures.len() as u32).unwrap()),
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
            // Output
            BindGroupLayoutEntry {
                binding: 4,
                visibility: ShaderStages::COMPUTE,
                ty: BindingType::StorageTexture {
                    access: StorageTextureAccess::ReadWrite,
                    // TODO: This should be selected by user. If they want to use 16bit textures, this should be rgba16, and convert
                    //       into target color space when merging down.
                    format: target_layer_texel.wgpu_format(),
                    view_dimension: TextureViewDimension::D2,
                },
                count: Some(NonZeroU32::new(output_len).unwrap()),
            },
        ];

        let mut external_var_bind_layout_entries = Vec::new();
        let mut cur_binding = EXTERNAL_VARIABLE_BASE_BINDING;
        for _ in 0..brush.external_vars().all().len() {
            external_var_bind_layout_entries.push(BindGroupLayoutEntry {
                binding: cur_binding,
                visibility: ShaderStages::COMPUTE,
                ty: BindingType::Buffer {
                    ty: BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            });
            cur_binding += 1;
        }
        main_layout_entries.extend(external_var_bind_layout_entries.clone());

        let main_layout = self
            .device
            .create_bind_group_layout(&BindGroupLayoutDescriptor {
                label: Some("brush main layout"),
                entries: &main_layout_entries,
            });

        let main_pipeline_layout = self
            .device
            .create_pipeline_layout(&PipelineLayoutDescriptor {
                label: Some("brush main pipeline layout"),
                bind_group_layouts: &[&main_layout],
                push_constant_ranges: &[],
            });

        let main_shader = self.device.create_shader_module(ShaderModuleDescriptor {
            label: Some("brush main shader"),
            source: ShaderSource::Wgsl(compiled_preset.main_graph.into()),
        });

        let main_pipeline = self
            .device
            .create_compute_pipeline(&ComputePipelineDescriptor {
                label: Some("brush main pipeline"),
                layout: Some(&main_pipeline_layout),
                module: &main_shader,
                entry_point: Some("main"),
                compilation_options: Default::default(),
                cache: None,
            });

        self.initialized = Some(InitializedData {
            estimated_size,
            output_len_in_layout: output_len,
            main_pipeline,
            main_graph_input: DynamicBuffer::default().with_usage(BufferUsages::STORAGE),
            stroke_info: DynamicBuffer::default().with_usage(BufferUsages::STORAGE),
            main_layout,
            target_layer_texel,
            textures,
            accumulated_area: IRect::EMPTY,
            compiled_stroke_postprocess_graphs: compiled_preset.stroke_postprocess_graphs,
            external_var_bind_layout_entries,
        });
    }

    pub fn prepare_main(
        &mut self,
        brush: &BrushPresetInstance,
        params: GraphInputParams,
        output_layer: LayerId,
        tiles: &GpuTileStorage,
    ) {
        let Some(initialized) = self.initialized.as_mut() else {
            return;
        };

        let estimated_area = brush.estimate_area(&params);
        initialized.accumulated_area = initialized.accumulated_area.union(estimated_area);

        initialized.main_graph_input.clear();
        initialized.main_graph_input.push(&GraphInputUniform {
            pen_position: params.pen_position,
        });
        initialized.main_graph_input.write_buffer(&self.device);

        initialized.stroke_info.clear();
        initialized.stroke_info.push(&StrokeInfoUniform {
            shader_origin: estimated_area.min,
            estimated_brush_size: initialized.estimated_size,
            tile_size: GpuTileStorageInner::TILE_SIZE,
        });
        initialized.stroke_info.write_buffer(&self.device);

        // Prepare tile info

        let tile_rect = GpuTileStorageInner::pixel_rect_to_tile(estimated_area);
        let mut tile_info = BufferVec::default().with_usage(BufferUsages::STORAGE);
        for y in tile_rect.min.y..tile_rect.max.y {
            for x in tile_rect.min.x..tile_rect.max.x {
                tile_info.push(&TileInfo {
                    tile_origin: IVec2::new(x, y) * GpuTileStorageInner::TILE_SIZE as i32,
                });
            }
        }
        tile_info.write_buffer(&self.device);

        // Referenced textures
        let texture_views = initialized
            .textures
            .iter()
            .map(std::convert::identity)
            .collect::<Vec<_>>();

        let mut main_bind_group_entries = vec![
            BindGroupEntry {
                binding: 0,
                resource: initialized.main_graph_input.binding().unwrap(),
            },
            BindGroupEntry {
                binding: 1,
                resource: initialized.stroke_info.binding().unwrap(),
            },
            BindGroupEntry {
                binding: 2,
                resource: BindingResource::TextureViewArray(&texture_views),
            },
            BindGroupEntry {
                binding: 3,
                // We can actually share the info buffer between main graph and postprocess graphs, because
                // get_tiles_mut_ordered returns tiles in the same order.
                resource: tile_info.binding().unwrap(),
            },
        ];

        // Prepare external variable buffers

        let mut external_var_buffers = Vec::new();
        for var in brush.external_vars().all().values() {
            let buffer = var.value.try_write_into_shader_buffer().unwrap();
            let gpu_buffer = self.device.create_buffer_init(&BufferInitDescriptor {
                label: Some("external variable buffer"),
                contents: &buffer,
                usage: BufferUsages::STORAGE,
            });
            external_var_buffers.push(gpu_buffer);
        }
        let mut external_var_bindings = Vec::with_capacity(external_var_buffers.len());
        let external_var_base_binding = EXTERNAL_VARIABLE_BASE_BINDING;
        for (index, buffer) in external_var_buffers.iter().enumerate() {
            external_var_bindings.push(BindGroupEntry {
                binding: external_var_base_binding + index as u32,
                resource: buffer.as_entire_binding(),
            });
        }

        // Prepare main bind group

        if brush.stroke_postprocess_graphs().is_empty() {
            // If no stroke postprocess, output the stroke directly to target layer.
            let output_surface = Self::generate_output_surface(
                tiles,
                output_layer,
                estimated_area,
                Some(initialized.output_len_in_layout),
                &self.device,
            );
            let output_layer_binding_array = Self::generate_texture_binding_array(&output_surface);

            main_bind_group_entries.push(BindGroupEntry {
                binding: 4,
                resource: BindingResource::TextureViewArray(&output_layer_binding_array),
            });
            main_bind_group_entries.extend(external_var_bindings);

            let main_bind_group = self.device.create_bind_group(&BindGroupDescriptor {
                label: Some("brush main bind group"),
                layout: &initialized.main_layout,
                entries: &main_bind_group_entries,
            });
            self.main_prepared = Some(MainPreparedData {
                main_bind_group,
                external_var_buffers,
                estimated_area,
            });
        } else {
            // Other wise, output to the ping pong buffer A.
            let pp_surface = Self::generate_output_surface(
                tiles,
                STROKE_INTERMEDIATE_SURFACE_A,
                estimated_area,
                Some(initialized.output_len_in_layout),
                &self.device,
            );

            let main_output_pp_surface = Self::generate_texture_binding_array(&pp_surface);

            let mut main_bind_group_entries = main_bind_group_entries.clone();
            main_bind_group_entries.push(BindGroupEntry {
                binding: 4,
                resource: BindingResource::TextureViewArray(&main_output_pp_surface),
            });
            main_bind_group_entries.extend(external_var_bindings);

            let main_bind_group = self.device.create_bind_group(&BindGroupDescriptor {
                label: Some("brush main bind group"),
                layout: &initialized.main_layout,
                entries: &main_bind_group_entries,
            });

            self.main_prepared = Some(MainPreparedData {
                main_bind_group,
                external_var_buffers,
                estimated_area,
            });
        }
    }

    pub fn prepare_stroke_postprocess(&mut self, tiles: &GpuTileStorage, target_layer: LayerId) {
        let (Some(initialized), Some(prepared)) =
            (self.initialized.as_mut(), self.main_prepared.as_ref())
        else {
            return;
        };

        let tile_rect = GpuTileStorageInner::pixel_rect_to_tile(initialized.accumulated_area);
        let output_surface = Self::generate_output_surface(
            tiles,
            target_layer,
            initialized.accumulated_area,
            None,
            &self.device,
        );
        let mut tile_info = BufferVec::default().with_usage(BufferUsages::STORAGE);
        for y in tile_rect.min.y..tile_rect.max.y {
            for x in tile_rect.min.x..tile_rect.max.x {
                tile_info.push(&TileInfo {
                    tile_origin: IVec2::new(x, y) * GpuTileStorageInner::TILE_SIZE as i32,
                });
            }
        }
        tile_info.write_buffer(&self.device);

        initialized.stroke_info.clear();
        initialized.stroke_info.push(&StrokeInfoUniform {
            shader_origin: initialized.accumulated_area.min,
            estimated_brush_size: initialized.accumulated_area.size().as_uvec2(),
            tile_size: GpuTileStorageInner::TILE_SIZE,
        });
        initialized.stroke_info.write_buffer(&self.device);

        let mut stroke_postprocess_layout_entries = vec![
            // Stroke Input Parameters
            BindGroupLayoutEntry {
                binding: 1,
                visibility: ShaderStages::COMPUTE,
                ty: BindingType::Buffer {
                    ty: BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: Some(StrokeInfoUniform::min_size()),
                },
                count: None,
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
                count: Some(NonZeroU32::new(initialized.textures.len() as u32).unwrap()),
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
            // Output
            BindGroupLayoutEntry {
                binding: 4,
                visibility: ShaderStages::COMPUTE,
                ty: BindingType::StorageTexture {
                    access: StorageTextureAccess::ReadWrite,
                    // TODO: This should be selected by user. If they want to use 16bit textures, this should be rgba16, and convert
                    //       into target color space when merging down.
                    format: initialized.target_layer_texel.wgpu_format(),
                    view_dimension: TextureViewDimension::D2,
                },
                count: Some(NonZeroU32::new(output_surface.len() as u32).unwrap()),
            },
            // Input
            BindGroupLayoutEntry {
                binding: 5,
                visibility: ShaderStages::COMPUTE,
                ty: BindingType::StorageTexture {
                    access: StorageTextureAccess::ReadOnly,
                    format: initialized.target_layer_texel.wgpu_format(),
                    view_dimension: TextureViewDimension::D2,
                },
                count: Some(NonZeroU32::new(output_surface.len() as u32).unwrap()),
            },
        ];
        stroke_postprocess_layout_entries
            .extend(initialized.external_var_bind_layout_entries.clone());

        let stroke_postprocess_layout =
            self.device
                .create_bind_group_layout(&BindGroupLayoutDescriptor {
                    label: Some("brush stroke postprocess layout"),
                    entries: &stroke_postprocess_layout_entries,
                });

        let stroke_postprocess_pipeline_layout =
            self.device
                .create_pipeline_layout(&PipelineLayoutDescriptor {
                    label: Some("brush stroke postprocess pipeline layout"),
                    bind_group_layouts: &[&stroke_postprocess_layout],
                    push_constant_ranges: &[],
                });

        let mut stroke_postprocess_pipelines = Vec::new();
        for shader in &initialized.compiled_stroke_postprocess_graphs {
            let shader = self.device.create_shader_module(ShaderModuleDescriptor {
                label: Some("brush stroke postprocess shader"),
                source: ShaderSource::Wgsl(shader.into()),
            });
            stroke_postprocess_pipelines.push(self.device.create_compute_pipeline(
                &ComputePipelineDescriptor {
                    label: Some("brush stroke postprocess pipeline"),
                    layout: Some(&stroke_postprocess_pipeline_layout),
                    module: &shader,
                    entry_point: Some("main"),
                    compilation_options: Default::default(),
                    cache: None,
                },
            ));
        }

        let texture_views = initialized
            .textures
            .iter()
            .map(std::convert::identity)
            .collect::<Vec<_>>();

        let bind_group_entries = vec![
            BindGroupEntry {
                binding: 1,
                resource: initialized.stroke_info.binding().unwrap(),
            },
            BindGroupEntry {
                binding: 2,
                resource: BindingResource::TextureViewArray(&texture_views),
            },
            BindGroupEntry {
                binding: 3,
                resource: tile_info.binding().unwrap(),
            },
        ];

        let total_stroke_pp = stroke_postprocess_pipelines.len();
        let mut stroke_postprocess_bind_groups = Vec::with_capacity(total_stroke_pp);
        let mut cur_pp_output_surface = 1;
        let pp_surfaces = [
            Self::generate_output_surface(
                tiles,
                STROKE_INTERMEDIATE_SURFACE_A,
                initialized.accumulated_area,
                None,
                &self.device,
            ),
            Self::generate_output_surface(
                tiles,
                STROKE_INTERMEDIATE_SURFACE_B,
                initialized.accumulated_area,
                None,
                &self.device,
            ),
        ];
        let pp_views = [
            Self::generate_texture_binding_array(&pp_surfaces[0]),
            Self::generate_texture_binding_array(&pp_surfaces[1]),
        ];
        let external_var_bindings = prepared
            .external_var_buffers
            .iter()
            .enumerate()
            .map(|(index, buffer)| BindGroupEntry {
                binding: EXTERNAL_VARIABLE_BASE_BINDING + index as u32,
                resource: buffer.as_entire_binding(),
            })
            .collect::<Vec<_>>();

        let output_view = Self::generate_texture_binding_array(&output_surface);

        for pp_index in 0..total_stroke_pp {
            let mut entries = bind_group_entries.clone();
            if pp_index == total_stroke_pp - 1 {
                entries.push(BindGroupEntry {
                    binding: 4,
                    resource: BindingResource::TextureViewArray(&output_view),
                });
            } else {
                entries.push(BindGroupEntry {
                    binding: 4,
                    resource: BindingResource::TextureViewArray(&pp_views[cur_pp_output_surface]),
                });
            }

            entries.push(BindGroupEntry {
                binding: 5,
                resource: BindingResource::TextureViewArray(&pp_views[1 - cur_pp_output_surface]),
            });

            entries.extend(external_var_bindings.clone());

            let stroke_pp_bind_group = self.device.create_bind_group(&BindGroupDescriptor {
                label: Some(&format!("brush stroke postprocess bind group {pp_index}")),
                layout: &stroke_postprocess_layout,
                entries: &entries,
            });
            stroke_postprocess_bind_groups.push(stroke_pp_bind_group);

            cur_pp_output_surface = (cur_pp_output_surface + 1) % 2;
        }

        self.stroke_postprocess_prepared = Some(StrokePostprocessPreparedData {
            pipelines: stroke_postprocess_pipelines,
            bind_groups: stroke_postprocess_bind_groups,
        });
    }

    pub fn draw(&self) {
        let (Some(initialized), Some(prepared)) =
            (self.initialized.as_ref(), self.main_prepared.as_ref())
        else {
            return;
        };

        let mut ec = self.device.create_command_encoder(&Default::default());

        {
            let mut pass = ec.begin_compute_pass(&Default::default());
            pass.set_pipeline(&initialized.main_pipeline);
            pass.set_bind_group(0, &prepared.main_bind_group, &[]);
            pass.dispatch_workgroups(
                initialized.estimated_size.x.div_ceil(16),
                initialized.estimated_size.y.div_ceil(16),
                1,
            );
        }

        self.queue.submit([ec.finish()]);
    }

    pub fn merge_down(&self) {
        let (Some(initialized), Some(stroke_pp_prepared)) = (
            self.initialized.as_ref(),
            self.stroke_postprocess_prepared.as_ref(),
        ) else {
            return;
        };

        let mut ec = self.device.create_command_encoder(&Default::default());

        for (pp_pipeline, pp_bind_group) in stroke_pp_prepared
            .pipelines
            .iter()
            .zip(stroke_pp_prepared.bind_groups.iter())
        {
            let mut pass = ec.begin_compute_pass(&Default::default());
            pass.set_pipeline(pp_pipeline);
            pass.set_bind_group(0, pp_bind_group, &[]);
            pass.dispatch_workgroups(
                (initialized.accumulated_area.width() as u32).div_ceil(16),
                (initialized.accumulated_area.height() as u32).div_ceil(16),
                1,
            );
        }

        self.queue.submit([ec.finish()]);
    }

    fn generate_output_surface(
        tiles_storage: &GpuTileStorage,
        layer: LayerId,
        estimated_area: IRect,
        output_len_in_layout: Option<u32>,
        device: &Device,
    ) -> Vec<Arc<TextureView>> {
        let format = tiles_storage.layer_texel_type(layer).unwrap().wgpu_format();
        let tiles = tiles_storage.get_tiles_mut_ordered(layer, estimated_area);
        let mut empty_placeholders = Vec::new();
        let mut output_layer = tiles.into_iter().map(|t| t.view).collect::<Vec<_>>();
        // Avoid partial binding.
        if let Some(expected) = output_len_in_layout {
            if output_layer.len() < expected as usize {
                for _ in output_layer.len()..expected as usize {
                    let view = device
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
                            format,
                            usage: TextureUsages::TEXTURE_BINDING | TextureUsages::STORAGE_BINDING,
                            view_formats: &[],
                        })
                        .create_view(&Default::default());
                    empty_placeholders.push(Arc::new(view));
                }
                output_layer.extend(empty_placeholders);
            }
        }

        output_layer
    }

    fn generate_texture_binding_array(textures: &[Arc<TextureView>]) -> Vec<&TextureView> {
        textures.iter().map(Arc::as_ref).collect()
    }
}
