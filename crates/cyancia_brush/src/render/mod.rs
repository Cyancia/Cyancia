use std::{collections::HashSet, num::NonZeroU32, sync::Arc};

use bevy_math::IRect;
use cyancia_assets::{asset::AssetId, store::AssetRegistry};
use cyancia_image::{
    layer::LayerId,
    texel::{TexelDepth, TexelFormat, TexelType},
    tile::{GpuLayerInfo, GpuTileInfo, GpuTileStorage, GpuTileStorageInner, Tile, TileIndex},
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
    ComputePipeline, ComputePipelineDescriptor, Device, Extent3d, Origin3d,
    PipelineLayoutDescriptor, Queue, ShaderModuleDescriptor, ShaderSource, ShaderStages,
    StorageTextureAccess, TexelCopyTextureInfo, Texture, TextureAspect, TextureDescriptor,
    TextureDimension, TextureFormat, TextureSampleType, TextureUsages, TextureView,
    TextureViewDimension,
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

const EXTERNAL_VARIABLE_BASE_BINDING: u32 = 9;

pub struct BrushPresetOperator {
    instance: Arc<BrushPresetInstance>,
    renderer: BrushPresetRenderer,
}

impl BrushPresetOperator {
    pub fn new(instance: Arc<BrushPresetInstance>, device: Arc<Device>, queue: Arc<Queue>) -> Self {
        let renderer = BrushPresetRenderer::new(device, queue);
        Self { instance, renderer }
    }

    pub fn begin_stroke(
        &mut self,
        tiles: &GpuTileStorage,
        assets: &AssetRegistry,
        target_layer: LayerId,
    ) {
        tiles.declare_layer(
            STROKE_INTERMEDIATE_SURFACE_A,
            GpuLayerInfo {
                texel_type: TexelType {
                    format: TexelFormat::Rgba,
                    depth: TexelDepth::Bit8,
                },
            },
        );
        tiles.declare_layer(
            STROKE_INTERMEDIATE_SURFACE_B,
            GpuLayerInfo {
                texel_type: TexelType {
                    format: TexelFormat::Rgba,
                    depth: TexelDepth::Bit8,
                },
            },
        );
        tiles.clear_layer(STROKE_INTERMEDIATE_SURFACE_A);
        tiles.clear_layer(STROKE_INTERMEDIATE_SURFACE_B);

        // TODO: Conditional reinitialize, this is kinda expensive.
        self.renderer
            .initialize(&mut self.instance, assets, target_layer, tiles);

        self.renderer.main_prepared = None;
        self.renderer.stroke_postprocess_prepared = None;
    }

    pub fn update_stroke(&mut self, params: GraphInputParams, tiles: &GpuTileStorage) {
        self.renderer
            .prepare_main(&mut self.instance, params, tiles);
        self.renderer.draw_main_dab(tiles);
        self.renderer.prepare_stroke_postprocess(tiles);
        self.renderer.postprocess_stroke();
        self.renderer.copy_last_surface_to_target(tiles);
    }

    pub fn end_stroke(&mut self, tiles: &GpuTileStorage) {
        // self.renderer.prepare_stroke_postprocess(tiles);
        // self.renderer.postprocess_stroke();
        // self.renderer.copy_last_surface_to_target(tiles);
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
}

struct InitializedData {
    target_layer: LayerId,
    referenced_textures: Vec<TextureView>,
    accumulated_area: IRect,
    compiled_main_graph: String,
    compiled_stroke_postprocess_graphs: Vec<String>,
    external_var_bind_layout_entries: Vec<BindGroupLayoutEntry>,
    graph_input: DynamicBuffer<GraphInputUniform>,
    stroke_info: DynamicBuffer<StrokeInfoUniform>,
    next_main_output_surface: usize,
    affected_tiles: HashSet<IVec2>,
}

struct MainPreparedData {
    estimated_area: IRect,
    main_pipeline: ComputePipeline,
    main_bind_group: BindGroup,
    external_var_buffers: Vec<Buffer>,
}

struct StrokePostprocessPreparedData {
    pipelines: Vec<ComputePipeline>,
    bind_groups: Vec<BindGroup>,
    last_surface: LayerId,
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
        target_layer: LayerId,
        tiles_storage: &GpuTileStorage,
    ) {
        let estimated_tile_count = GpuTileStorageInner::calc_tile_count(brush.estimate_size()) + 2;
        let buffer_len = estimated_tile_count.element_product();
        // TODO: Handle shader compile error
        let compiled_preset = brush.compile(EXTERNAL_VARIABLE_BASE_BINDING).unwrap();
        println!("Compiled brush preset: \n{compiled_preset}");

        // Prepare referenced textures

        let mut referenced_textures = Vec::new();
        for id in compiled_preset.texture_usages {
            if id == TextureId::NULL {
                referenced_textures
                    .push(self.empty_texture.texture.create_view(&Default::default()));
                continue;
            }

            let handle = assets.handle(AssetId::new(*id)).unwrap();
            let gpu_image = GpuImage::from_asset(
                &self.device,
                &self.queue,
                &handle.get().unwrap(),
                TextureUsages::TEXTURE_BINDING,
            );
            referenced_textures.push(gpu_image.texture.create_view(&Default::default()));
        }

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

        self.initialized = Some(InitializedData {
            graph_input: DynamicBuffer::default().with_usage(BufferUsages::STORAGE),
            stroke_info: DynamicBuffer::default().with_usage(BufferUsages::STORAGE),
            referenced_textures,
            accumulated_area: IRect::EMPTY,
            compiled_main_graph: compiled_preset.main_graph,
            compiled_stroke_postprocess_graphs: compiled_preset.stroke_postprocess_graphs,
            external_var_bind_layout_entries,
            target_layer,
            next_main_output_surface: 0,
            affected_tiles: HashSet::new(),
        });
    }

    pub fn prepare_main(
        &mut self,
        brush: &BrushPresetInstance,
        params: GraphInputParams,
        tiles: &GpuTileStorage,
    ) {
        let Some(InitializedData {
            target_layer,
            referenced_textures,
            accumulated_area,
            compiled_main_graph,
            compiled_stroke_postprocess_graphs,
            external_var_bind_layout_entries,
            graph_input,
            stroke_info,
            next_main_output_surface,
            affected_tiles,
        }) = self.initialized.as_mut()
        else {
            return;
        };

        let estimated_area = GpuTileStorageInner::snap_to_tile_grid(brush.estimate_area(&params));
        *accumulated_area = accumulated_area.union(estimated_area);

        let affected = GpuTileStorageInner::pixel_rect_to_tile(estimated_area);
        for y in affected.min.y..affected.max.y {
            for x in affected.min.x..affected.max.x {
                affected_tiles.insert(IVec2::new(x, y));
            }
        }

        let target_layer_texel = tiles.get_layer_info(*target_layer).unwrap().texel_type;

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
                count: Some(NonZeroU32::new(referenced_textures.len() as u32).unwrap()),
            },
            // Target Layer Tile Info
            BindGroupLayoutEntry {
                binding: 3,
                visibility: ShaderStages::COMPUTE,
                ty: BindingType::Buffer {
                    ty: BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: Some(GpuTileInfo::min_size()),
                },
                count: None,
            },
            // Target layer
            BindGroupLayoutEntry {
                binding: 4,
                visibility: ShaderStages::COMPUTE,
                ty: BindingType::StorageTexture {
                    access: StorageTextureAccess::ReadOnly,
                    format: target_layer_texel.wgpu_format(),
                    view_dimension: TextureViewDimension::D2Array,
                },
                count: None,
            },
            // Output Tile Info
            BindGroupLayoutEntry {
                binding: 5,
                visibility: ShaderStages::COMPUTE,
                ty: BindingType::Buffer {
                    ty: BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: Some(GpuTileInfo::min_size()),
                },
                count: None,
            },
            // Output
            BindGroupLayoutEntry {
                binding: 6,
                visibility: ShaderStages::COMPUTE,
                ty: BindingType::StorageTexture {
                    access: StorageTextureAccess::WriteOnly,
                    // TODO: This should be selected by user. If they want to use 16bit textures, this should be rgba16, and convert
                    //       into target color space when merging down.
                    format: target_layer_texel.wgpu_format(),
                    view_dimension: TextureViewDimension::D2Array,
                },
                count: None,
            },
            // Input Tile Info
            BindGroupLayoutEntry {
                binding: 7,
                visibility: ShaderStages::COMPUTE,
                ty: BindingType::Buffer {
                    ty: BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: Some(GpuTileInfo::min_size()),
                },
                count: None,
            },
            // Input
            BindGroupLayoutEntry {
                binding: 8,
                visibility: ShaderStages::COMPUTE,
                ty: BindingType::StorageTexture {
                    access: StorageTextureAccess::ReadOnly,
                    // TODO: This should be selected by user. If they want to use 16bit textures, this should be rgba16, and convert
                    //       into target color space when merging down.
                    format: target_layer_texel.wgpu_format(),
                    view_dimension: TextureViewDimension::D2Array,
                },
                count: None,
            },
        ];
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
            source: ShaderSource::Wgsl(compiled_main_graph.clone().into()),
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

        // Prepare buffers

        graph_input.clear();
        graph_input.push(&GraphInputUniform {
            pen_position: params.pen_position,
        });
        graph_input.write_buffer(&self.device);

        stroke_info.clear();
        stroke_info.push(&StrokeInfoUniform {
            shader_origin: estimated_area.min,
            estimated_brush_size: estimated_area.size().as_uvec2(),
        });
        stroke_info.write_buffer(&self.device);

        // Referenced textures
        let referenced_texture_views = referenced_textures
            .iter()
            .map(std::convert::identity)
            .collect::<Vec<_>>();

        // Layer bindings
        let target_layer = tiles.get_layer_binding_or_empty(*target_layer).unwrap();
        let mut pp_layers = [
            tiles.get_layer_mut(STROKE_INTERMEDIATE_SURFACE_A).unwrap(),
            tiles.get_layer_mut(STROKE_INTERMEDIATE_SURFACE_B).unwrap(),
        ];
        pp_layers[0].ensure_pixel_area(*accumulated_area);
        pp_layers[1].ensure_pixel_area(*accumulated_area);
        let pp_textures = [
            pp_layers[0].texture().unwrap(),
            pp_layers[1].texture().unwrap(),
        ];
        let pp_info_buffers = [
            pp_layers[0].tile_info_buffer().unwrap(),
            pp_layers[1].tile_info_buffer().unwrap(),
        ];

        // Prepare main bind group
        let mut main_bind_group_entries = vec![
            BindGroupEntry {
                binding: 0,
                resource: graph_input.binding().unwrap(),
            },
            BindGroupEntry {
                binding: 1,
                resource: stroke_info.binding().unwrap(),
            },
            BindGroupEntry {
                binding: 2,
                resource: BindingResource::TextureViewArray(&referenced_texture_views),
            },
            BindGroupEntry {
                binding: 3,
                resource: target_layer.tile_info_buffer.as_entire_binding(),
            },
            BindGroupEntry {
                binding: 4,
                resource: BindingResource::TextureView(target_layer.texture.as_ref()),
            },
            BindGroupEntry {
                binding: 5,
                resource: pp_info_buffers[*next_main_output_surface].as_entire_binding(),
            },
            BindGroupEntry {
                binding: 6,
                resource: BindingResource::TextureView(
                    pp_textures[*next_main_output_surface].as_ref(),
                ),
            },
            BindGroupEntry {
                binding: 7,
                resource: pp_info_buffers[1 - *next_main_output_surface].as_entire_binding(),
            },
            BindGroupEntry {
                binding: 8,
                resource: BindingResource::TextureView(
                    pp_textures[1 - *next_main_output_surface].as_ref(),
                ),
            },
        ];

        // Prepare external variable buffers and layout entries

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

        main_bind_group_entries.extend(external_var_bindings);
        *next_main_output_surface = 1 - *next_main_output_surface;

        let main_bind_group = self.device.create_bind_group(&BindGroupDescriptor {
            label: Some("brush main bind group"),
            layout: &main_layout,
            entries: &main_bind_group_entries,
        });
        self.main_prepared = Some(MainPreparedData {
            estimated_area,
            main_pipeline,
            main_bind_group,
            external_var_buffers,
        });
    }

    pub fn prepare_stroke_postprocess(&mut self, tiles: &GpuTileStorage) {
        let (Some(initialized), Some(prepared)) =
            (self.initialized.as_mut(), self.main_prepared.as_ref())
        else {
            return;
        };

        let target_layer_tiles = tiles
            .get_layer_binding_or_empty(initialized.target_layer)
            .unwrap();
        let target_layer_texel = tiles
            .get_layer_info(initialized.target_layer)
            .unwrap()
            .texel_type;
        let pp_layers = [
            tiles
                .get_layer_binding_or_empty(STROKE_INTERMEDIATE_SURFACE_A)
                .unwrap(),
            tiles
                .get_layer_binding_or_empty(STROKE_INTERMEDIATE_SURFACE_B)
                .unwrap(),
        ];

        initialized.stroke_info.clear();
        initialized.stroke_info.push(&StrokeInfoUniform {
            shader_origin: initialized.accumulated_area.min,
            estimated_brush_size: initialized.accumulated_area.size().as_uvec2(),
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
                count: Some(NonZeroU32::new(initialized.referenced_textures.len() as u32).unwrap()),
            },
            // Target layer tile info
            BindGroupLayoutEntry {
                binding: 3,
                visibility: ShaderStages::COMPUTE,
                ty: BindingType::Buffer {
                    ty: BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: Some(GpuTileInfo::min_size()),
                },
                count: None,
            },
            // Target layer
            BindGroupLayoutEntry {
                binding: 4,
                visibility: ShaderStages::COMPUTE,
                ty: BindingType::StorageTexture {
                    access: StorageTextureAccess::ReadOnly,
                    format: target_layer_texel.wgpu_format(),
                    view_dimension: TextureViewDimension::D2Array,
                },
                count: None,
            },
            // Output tile info
            BindGroupLayoutEntry {
                binding: 5,
                visibility: ShaderStages::COMPUTE,
                ty: BindingType::Buffer {
                    ty: BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: Some(GpuTileInfo::min_size()),
                },
                count: None,
            },
            // Output
            BindGroupLayoutEntry {
                binding: 6,
                visibility: ShaderStages::COMPUTE,
                ty: BindingType::StorageTexture {
                    access: StorageTextureAccess::WriteOnly,
                    // TODO: This should be selected by user. If they want to use 16bit textures, this should be rgba16, and convert
                    //       into target color space when merging down.
                    format: target_layer_texel.wgpu_format(),
                    view_dimension: TextureViewDimension::D2Array,
                },
                count: None,
            },
            // Input tile info
            BindGroupLayoutEntry {
                binding: 7,
                visibility: ShaderStages::COMPUTE,
                ty: BindingType::Buffer {
                    ty: BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: Some(GpuTileInfo::min_size()),
                },
                count: None,
            },
            // Input
            BindGroupLayoutEntry {
                binding: 8,
                visibility: ShaderStages::COMPUTE,
                ty: BindingType::StorageTexture {
                    access: StorageTextureAccess::ReadOnly,
                    format: target_layer_texel.wgpu_format(),
                    view_dimension: TextureViewDimension::D2Array,
                },
                count: None,
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

        let referenced_texture_views = initialized
            .referenced_textures
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
                resource: BindingResource::TextureViewArray(&referenced_texture_views),
            },
            BindGroupEntry {
                binding: 3,
                resource: target_layer_tiles.tile_info_buffer.as_entire_binding(),
            },
            BindGroupEntry {
                binding: 4,
                resource: BindingResource::TextureView(target_layer_tiles.texture.as_ref()),
            },
        ];

        let total_stroke_pp = stroke_postprocess_pipelines.len();
        let mut stroke_postprocess_bind_groups = Vec::with_capacity(total_stroke_pp);
        let mut next_pp_output_surface = initialized.next_main_output_surface;

        let external_var_bindings = prepared
            .external_var_buffers
            .iter()
            .enumerate()
            .map(|(index, buffer)| BindGroupEntry {
                binding: EXTERNAL_VARIABLE_BASE_BINDING + index as u32,
                resource: buffer.as_entire_binding(),
            })
            .collect::<Vec<_>>();

        for pp_index in 0..total_stroke_pp {
            let mut entries = bind_group_entries.clone();
            let output = &pp_layers[next_pp_output_surface];
            let input = &pp_layers[1 - next_pp_output_surface];
            entries.extend([
                BindGroupEntry {
                    binding: 5,
                    resource: output.tile_info_buffer.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 6,
                    resource: BindingResource::TextureView(&output.texture),
                },
                BindGroupEntry {
                    binding: 7,
                    resource: input.tile_info_buffer.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 8,
                    resource: BindingResource::TextureView(&input.texture),
                },
            ]);

            entries.extend(external_var_bindings.clone());

            let stroke_pp_bind_group = self.device.create_bind_group(&BindGroupDescriptor {
                label: Some(&format!("brush stroke postprocess bind group {pp_index}")),
                layout: &stroke_postprocess_layout,
                entries: &entries,
            });
            stroke_postprocess_bind_groups.push(stroke_pp_bind_group);

            next_pp_output_surface = (next_pp_output_surface + 1) % 2;
        }

        self.stroke_postprocess_prepared = Some(StrokePostprocessPreparedData {
            pipelines: stroke_postprocess_pipelines,
            bind_groups: stroke_postprocess_bind_groups,
            last_surface: if next_pp_output_surface == 0 {
                STROKE_INTERMEDIATE_SURFACE_B
            } else {
                STROKE_INTERMEDIATE_SURFACE_A
            },
        });
    }

    pub fn draw_main_dab(&self, _tiles: &GpuTileStorage) {
        let (Some(initialized), Some(prepared)) =
            (self.initialized.as_ref(), self.main_prepared.as_ref())
        else {
            return;
        };

        let mut ec = self.device.create_command_encoder(&Default::default());

        {
            let mut pass = ec.begin_compute_pass(&Default::default());
            pass.set_pipeline(&prepared.main_pipeline);
            pass.set_bind_group(0, &prepared.main_bind_group, &[]);
            let size = prepared.estimated_area.size().as_uvec2();
            pass.dispatch_workgroups(size.x.div_ceil(16), size.y.div_ceil(16), 1);
        }

        self.queue.submit([ec.finish()]);
    }

    pub fn copy_last_surface_to_target(&self, tiles: &GpuTileStorage) {
        let (Some(initialized), Some(stroke_pp_prepared)) = (
            self.initialized.as_ref(),
            self.stroke_postprocess_prepared.as_ref(),
        ) else {
            return;
        };

        let result_layer = tiles.get_layer(stroke_pp_prepared.last_surface).unwrap();
        let mut target_layer = tiles.get_layer_mut(initialized.target_layer).unwrap();

        let mut ec = self.device.create_command_encoder(&Default::default());
        for index in &initialized.affected_tiles {
            let src_layer = result_layer.get_tile_layer(*index).unwrap();
            target_layer.get_tile_or_allocate(*index);
            let dst_layer = target_layer.get_tile_layer(*index).unwrap();

            ec.copy_texture_to_texture(
                TexelCopyTextureInfo {
                    texture: result_layer.texture().unwrap().texture(),
                    mip_level: 0,
                    origin: Origin3d {
                        x: 0,
                        y: 0,
                        z: src_layer,
                    },
                    aspect: TextureAspect::All,
                },
                TexelCopyTextureInfo {
                    texture: target_layer.texture().unwrap().texture(),
                    mip_level: 0,
                    origin: Origin3d {
                        x: 0,
                        y: 0,
                        z: dst_layer,
                    },
                    aspect: TextureAspect::All,
                },
                Extent3d {
                    width: GpuTileStorageInner::TILE_SIZE,
                    height: GpuTileStorageInner::TILE_SIZE,
                    depth_or_array_layers: 1,
                },
            );
        }

        self.queue.submit([ec.finish()]);
    }

    pub fn postprocess_stroke(&self) {
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
}
