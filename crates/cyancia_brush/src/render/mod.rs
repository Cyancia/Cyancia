use std::{
    collections::{HashSet, VecDeque},
    num::NonZeroU32,
    sync::Arc,
};

use bevy_math::IRect;
use cyancia_assets::{asset::AssetId, store::AssetRegistry};
use cyancia_image::{
    layer::LayerId,
    texel::{TexelDepth, TexelFormat, TexelType},
    tile::{GpuLayerInfo, GpuTileInfo, GpuTileStorage, GpuTileStorageInner, Tile, TileIndex},
};
use cyancia_input::mouse::PressedMouseState;
use cyancia_math::number::LerpAngle;
use cyancia_render::buffer::{BufferVec, DynamicBuffer};
use cyancia_shader_graph::graph::texture::TextureId;
use cyancia_utils::include_shader;
use encase::{ShaderType, StorageBuffer};
use glam::{IVec2, IVec4, UVec2, UVec3, UVec4, Vec2, Vec4Swizzles};
use parking_lot::RwLock;
use ringbuffer::{AllocRingBuffer, RingBuffer};
use uuid::Uuid;
use wesl::{VirtualResolver, Wesl};
use wgpu::{
    BindGroup, BindGroupDescriptor, BindGroupEntry, BindGroupLayout, BindGroupLayoutDescriptor,
    BindGroupLayoutEntry, BindingResource, BindingType, Buffer, BufferBindingType,
    BufferDescriptor, BufferUsages, CommandEncoder, ComputePipeline, ComputePipelineDescriptor,
    Device, Extent3d, MapMode, Origin3d, PipelineLayoutDescriptor, Queue, ShaderModuleDescriptor,
    ShaderSource, ShaderStages, StorageTextureAccess, TexelCopyTextureInfo, Texture, TextureAspect,
    TextureDescriptor, TextureDimension, TextureFormat, TextureSampleType, TextureUsages,
    TextureView, TextureViewDimension,
    naga::StorageAccess,
    util::{BufferInitDescriptor, DeviceExt},
    wgt::PollType,
};

use crate::{
    asset::{BrushPresetInstance, CompiledBrushGraph, CompiledBrushPreset, GpuImage},
    render::{
        dynamic_intermediate_buffer::{DynamicGpuTileInfoBuffer, DynamicIntermediateBuffer},
        graph::GraphInputParams,
    },
};

pub mod dynamic_intermediate_buffer;
pub mod graph;

const EXTERNAL_VARIABLE_BASE_BINDING: u32 = 32;

pub struct BrushPresetOperator {
    instance: Arc<RwLock<BrushPresetInstance>>,
    renderer: BrushPresetRenderer,
    sampler: StrokePenInputSampler,
}

impl BrushPresetOperator {
    pub fn new(
        instance: Arc<RwLock<BrushPresetInstance>>,
        device: Arc<Device>,
        queue: Arc<Queue>,
    ) -> Self {
        let renderer = BrushPresetRenderer::new(device, queue);

        Self {
            instance,
            renderer,
            sampler: StrokePenInputSampler::new(), // TODO dynamic spacing
        }
    }

    pub fn begin_stroke(
        &mut self,
        input: PenInputSample,
        tiles: &GpuTileStorage,
        assets: &AssetRegistry,
        target_layer: LayerId,
    ) {
        self.sampler = StrokePenInputSampler::new();
        self.sampler.input(input);

        // TODO: Conditional reinitialize, this is kinda expensive.
        let instance = self.instance.read();
        self.renderer
            .initialize(&instance, assets, target_layer, tiles);
        self.renderer.prepare(&instance, tiles);
    }

    pub fn update_stroke(&mut self, input: PenInputSample, tiles: &GpuTileStorage) {
        self.sampler.input(input);
        let mut ec = self
            .renderer
            .device
            .create_command_encoder(&Default::default());

        let now = std::time::Instant::now();
        let mut n_samples = 0;
        for (i_sample, sample) in self.sampler.drain_samples().into_iter().enumerate() {
            let params = GraphInputParams {
                pen_position: sample.position,
                draw_direction_vec: sample.draw_direction_vec,
                draw_direction_angle: sample.draw_direction_angle,
            };
            ec.push_debug_group(&format!("Draw sample {i_sample}"));
            self.renderer.draw_main(params, tiles, &mut ec);
            ec.pop_debug_group();
            n_samples += 1;
        }

        self.renderer.queue.submit([ec.finish()]);

        log::info!(
            "Draw main graph: {} ms (avg {}ms each, {} samples)",
            now.elapsed().as_secs_f32() * 1000.0,
            now.elapsed().as_secs_f32() * 1000.0 / (n_samples as f32),
            n_samples
        );
    }

    pub fn end_stroke(&mut self, tiles: &GpuTileStorage) {
        let now = std::time::Instant::now();
        self.renderer.draw_stroke_postprocess(tiles);
        self.renderer.copy_last_surface_to_target(tiles);
        log::info!(
            "Draw stroke postprocess and copy: {} ms",
            now.elapsed().as_secs_f32() * 1000.0
        );
    }
}

pub struct PenInputSample {
    pub position: Vec2,
}

#[derive(Default, Clone, Copy)]
pub struct ComputedPenInputSample {
    pub position: Vec2,
    pub draw_direction_vec: Vec2,
    pub draw_direction_angle: f32,
}

impl ComputedPenInputSample {
    pub fn lerp(&self, other: &Self, t: f32) -> Self {
        let draw_direction_angle = self
            .draw_direction_angle
            .lerp_angle(other.draw_direction_angle, t);
        let (draw_direction_angle_sin, draw_direction_angle_cos) = draw_direction_angle.sin_cos();
        Self {
            position: self.position.lerp(other.position, t),
            draw_direction_vec: Vec2::new(draw_direction_angle_cos, draw_direction_angle_sin),
            draw_direction_angle,
        }
    }
}

pub struct StrokePenInputSampler {
    spacing: f32,
    samples: AllocRingBuffer<ComputedPenInputSample>,
    queued: VecDeque<ComputedPenInputSample>,
}

impl StrokePenInputSampler {
    pub fn new() -> Self {
        Self {
            spacing: 1.0,
            samples: AllocRingBuffer::new(2),
            queued: VecDeque::new(),
        }
    }

    pub fn set_spacing(&mut self, spacing: f32) {
        self.spacing = spacing.max(1.0);
    }

    pub fn input(&mut self, mouse: PenInputSample) {
        if let Some(last) = self.samples.front().cloned() {
            let draw_direction_angle = (mouse.position - last.position).angle_to(Vec2::X);
            let (draw_direction_angle_sin, draw_direction_angle_cos) =
                draw_direction_angle.sin_cos();
            let this = ComputedPenInputSample {
                position: mouse.position,
                draw_direction_vec: Vec2::new(draw_direction_angle_cos, draw_direction_angle_sin),
                draw_direction_angle,
            };
            self.samples.enqueue(this);

            let total_dist = this.position.distance(last.position);
            let mut cur_dist = total_dist;
            while cur_dist >= self.spacing {
                let t = 1.0 - (cur_dist / total_dist);
                self.queued.push_back(last.lerp(&this, t));
                cur_dist -= self.spacing;
            }
        } else {
            self.samples.enqueue(ComputedPenInputSample {
                position: mouse.position,
                draw_direction_vec: Vec2::X,
                draw_direction_angle: 0.0,
            });
        }
    }

    pub fn drain_samples(&mut self) -> Vec<ComputedPenInputSample> {
        let mut result = Vec::new();
        while let Some(sample) = self.queued.pop_front() {
            result.push(sample);
        }
        result
    }
}

#[derive(ShaderType, Debug)]
pub struct GraphInputUniform {
    pub pen_position: Vec2,
}

#[derive(ShaderType, Debug, Default)]
pub struct StrokeInfoUniform {
    pub bound_min: IVec2,
    pub bound_max: IVec2,
    pub accumulated_bound_min: IVec2,
    pub accumulated_bound_max: IVec2,
}

struct InitializedData {
    target_layer: LayerId,
    referenced_textures: Vec<TextureView>,
    intermediate_buffers: DynamicIntermediateBuffer,

    graph_input: Buffer,
    stroke_info: Buffer,

    main_pipeline: ComputePipeline,
    main_layout: BindGroupLayout,
    main_esti_pipeline: ComputePipeline,
    main_esti_layout: BindGroupLayout,
    stroke_pp_pipelines: Vec<ComputePipeline>,
    stroke_pp_layout: BindGroupLayout,
    stroke_pp_esti_pipelines: Vec<ComputePipeline>,
    stroke_pp_esti_layout: BindGroupLayout,
}

struct PreparedData {
    external_var_buffers: Vec<Buffer>,

    tile_allocation_bind_group: BindGroup,

    main_bind_groups: [BindGroup; 2],
    main_esti_bind_groups: [BindGroup; 2],

    next_bind_group: usize,
}

pub struct BrushPresetRenderer {
    device: Arc<Device>,
    queue: Arc<Queue>,

    initialized: Option<InitializedData>,
    prepared: Option<PreparedData>,

    tile_allocation_dispatch: Buffer,
    main_dispatch: Buffer,

    tile_allocation_layout: BindGroupLayout,
    tile_allocation_pipeline: ComputePipeline,

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

        // Prepare buffers

        let mut tile_allocation_dispatch = DynamicBuffer::new(
            Some("tile allocation dispatch"),
            BufferUsages::STORAGE | BufferUsages::INDIRECT,
        );
        tile_allocation_dispatch.push(&UVec4::ZERO);
        tile_allocation_dispatch.write_buffer(&device);

        let mut main_dispatch = DynamicBuffer::new(
            Some("main dispatch"),
            BufferUsages::STORAGE | BufferUsages::INDIRECT,
        );
        main_dispatch.push(&UVec4::ZERO);
        main_dispatch.write_buffer(&device);

        let tile_allocation_layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some("brush tile allocation bind group layout"),
            entries: &[
                // Estimated affected tiles
                BindGroupLayoutEntry {
                    binding: 0,
                    visibility: ShaderStages::COMPUTE,
                    ty: BindingType::Buffer {
                        ty: BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: Some(StrokeInfoUniform::min_size()),
                    },
                    count: None,
                },
                // Tile info
                BindGroupLayoutEntry {
                    binding: 1,
                    visibility: ShaderStages::COMPUTE,
                    ty: BindingType::Buffer {
                        ty: BufferBindingType::Storage { read_only: false },
                        has_dynamic_offset: false,
                        min_binding_size: Some(DynamicGpuTileInfoBuffer::min_size()),
                    },
                    count: None,
                },
            ],
        });

        let tile_allocation_pipeline_layout =
            device.create_pipeline_layout(&PipelineLayoutDescriptor {
                label: Some("brush tile allocation pipeline layout"),
                bind_group_layouts: &[&tile_allocation_layout],
                push_constant_ranges: &[],
            });

        let tile_allocation_shader = device.create_shader_module(ShaderModuleDescriptor {
            label: Some("brush tile allocation shader"),
            source: ShaderSource::Wgsl(include_shader!("brush_tile_allocation.wgsl").into()),
        });

        let tile_allocation_pipeline = device.create_compute_pipeline(&ComputePipelineDescriptor {
            label: Some("brush tile allocation pipeline"),
            layout: Some(&tile_allocation_pipeline_layout),
            module: &tile_allocation_shader,
            entry_point: Some("allocate"),
            compilation_options: Default::default(),
            cache: None,
        });

        Self {
            device,
            queue,
            empty_texture: GpuImage {
                texture: empty_texture,
            },

            main_dispatch: main_dispatch.inner_buffer().unwrap().clone(),
            tile_allocation_dispatch: tile_allocation_dispatch.inner_buffer().unwrap().clone(),

            tile_allocation_layout,
            tile_allocation_pipeline,

            initialized: None,
            prepared: None,
        }
    }

    pub fn initialize(
        &mut self,
        brush: &BrushPresetInstance,
        assets: &AssetRegistry,
        target_layer: LayerId,
        tiles_storage: &GpuTileStorage,
    ) {
        // TODO: Handle shader compile error
        let compiled_preset = brush.compile(EXTERNAL_VARIABLE_BASE_BINDING).unwrap();
        println!("Compiled brush preset: \n{compiled_preset}");

        // Prepare intermediate buffers

        let layer_info = tiles_storage.get_layer_info(target_layer).unwrap();
        let intermediate_buffers =
            DynamicIntermediateBuffer::new(256, layer_info.texel_type, self.device.clone());

        let mut stroke_info = DynamicBuffer::new(Some("stroke info buffer"), BufferUsages::STORAGE);
        stroke_info.push(&StrokeInfoUniform {
            accumulated_bound_min: IVec2::splat(i32::MAX),
            accumulated_bound_max: IVec2::splat(i32::MIN),
            ..Default::default()
        });
        stroke_info.write_buffer(&self.device);

        // Prepare referenced textures

        let mut referenced_textures = Vec::new();
        for id in compiled_preset.texture_usage {
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

        // Prepare external variable bind group layout entries

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

        let target_layer_texel = tiles_storage
            .get_layer_info(target_layer)
            .unwrap()
            .texel_type;

        // Prepare main bind group layout and pipeline

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
                    ty: BufferBindingType::Storage { read_only: false },
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
                    min_binding_size: Some(DynamicGpuTileInfoBuffer::min_size()),
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
                    min_binding_size: Some(DynamicGpuTileInfoBuffer::min_size()),
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
        let mut main_esti_layout_entries = vec![
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
                    ty: BufferBindingType::Storage { read_only: false },
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
            // Input Tile Info
            BindGroupLayoutEntry {
                binding: 7,
                visibility: ShaderStages::COMPUTE,
                ty: BindingType::Buffer {
                    ty: BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: Some(DynamicGpuTileInfoBuffer::min_size()),
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
            // Tile allocation dispatch
            BindGroupLayoutEntry {
                binding: 16,
                visibility: ShaderStages::COMPUTE,
                ty: BindingType::Buffer {
                    ty: BufferBindingType::Storage { read_only: false },
                    has_dynamic_offset: false,
                    min_binding_size: Some(UVec3::min_size()),
                },
                count: None,
            },
            // Main dispatch
            BindGroupLayoutEntry {
                binding: 17,
                visibility: ShaderStages::COMPUTE,
                ty: BindingType::Buffer {
                    ty: BufferBindingType::Storage { read_only: false },
                    has_dynamic_offset: false,
                    min_binding_size: Some(UVec3::min_size()),
                },
                count: None,
            },
        ];
        main_esti_layout_entries.extend(external_var_bind_layout_entries.clone());

        let main_layout = self
            .device
            .create_bind_group_layout(&BindGroupLayoutDescriptor {
                label: Some("brush main layout"),
                entries: &main_layout_entries,
            });
        let main_esti_layout = self
            .device
            .create_bind_group_layout(&BindGroupLayoutDescriptor {
                label: Some("brush main estimation layout"),
                entries: &main_esti_layout_entries,
            });

        let main_pipeline_layout = self
            .device
            .create_pipeline_layout(&PipelineLayoutDescriptor {
                label: Some("brush main pipeline layout"),
                bind_group_layouts: &[&main_layout],
                push_constant_ranges: &[],
            });
        let main_esti_pipeline_layout =
            self.device
                .create_pipeline_layout(&PipelineLayoutDescriptor {
                    label: Some("brush main estimation pipeline layout"),
                    bind_group_layouts: &[&main_esti_layout],
                    push_constant_ranges: &[],
                });

        let main_shader = self.device.create_shader_module(ShaderModuleDescriptor {
            label: Some("brush main shader"),
            source: ShaderSource::Wgsl(compiled_preset.main_graph.shader.clone().into()),
        });
        let main_esti_shader = self.device.create_shader_module(ShaderModuleDescriptor {
            label: Some("brush main estimation shader"),
            source: ShaderSource::Wgsl(compiled_preset.main_graph.size_estimation.clone().into()),
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
        let main_esti_pipeline = self
            .device
            .create_compute_pipeline(&ComputePipelineDescriptor {
                label: Some("brush main estimation pipeline"),
                layout: Some(&main_esti_pipeline_layout),
                module: &main_esti_shader,
                entry_point: Some("estimate"),
                compilation_options: Default::default(),
                cache: None,
            });

        // Prepare stroke postprocess bind group layout and pipelines

        let mut stroke_pp_layout_entries = vec![
            // Stroke Input Parameters
            BindGroupLayoutEntry {
                binding: 1,
                visibility: ShaderStages::COMPUTE,
                ty: BindingType::Buffer {
                    ty: BufferBindingType::Storage { read_only: false },
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
                    min_binding_size: Some(DynamicGpuTileInfoBuffer::min_size()),
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
                    min_binding_size: Some(DynamicGpuTileInfoBuffer::min_size()),
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
        stroke_pp_layout_entries.extend(external_var_bind_layout_entries.clone());
        let mut stroke_pp_esti_layout_entries = vec![
            // Stroke Info
            BindGroupLayoutEntry {
                binding: 1,
                visibility: ShaderStages::COMPUTE,
                ty: BindingType::Buffer {
                    ty: BufferBindingType::Storage { read_only: false },
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
            // Input Tile Info
            BindGroupLayoutEntry {
                binding: 7,
                visibility: ShaderStages::COMPUTE,
                ty: BindingType::Buffer {
                    ty: BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: Some(DynamicGpuTileInfoBuffer::min_size()),
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
            // Tile allocation dispatch
            BindGroupLayoutEntry {
                binding: 16,
                visibility: ShaderStages::COMPUTE,
                ty: BindingType::Buffer {
                    ty: BufferBindingType::Storage { read_only: false },
                    has_dynamic_offset: false,
                    min_binding_size: Some(UVec3::min_size()),
                },
                count: None,
            },
            // Main dispatch
            BindGroupLayoutEntry {
                binding: 17,
                visibility: ShaderStages::COMPUTE,
                ty: BindingType::Buffer {
                    ty: BufferBindingType::Storage { read_only: false },
                    has_dynamic_offset: false,
                    min_binding_size: Some(UVec3::min_size()),
                },
                count: None,
            },
        ];
        stroke_pp_esti_layout_entries.extend(external_var_bind_layout_entries.clone());

        let stroke_pp_layout = self
            .device
            .create_bind_group_layout(&BindGroupLayoutDescriptor {
                label: Some("brush stroke postprocess layout"),
                entries: &stroke_pp_layout_entries,
            });
        let stroke_pp_esti_layout =
            self.device
                .create_bind_group_layout(&BindGroupLayoutDescriptor {
                    label: Some("brush stroke postprocess estimation layout"),
                    entries: &stroke_pp_esti_layout_entries,
                });

        let stroke_pp_pipeline_layout =
            self.device
                .create_pipeline_layout(&PipelineLayoutDescriptor {
                    label: Some("brush stroke postprocess pipeline layout"),
                    bind_group_layouts: &[&stroke_pp_layout],
                    push_constant_ranges: &[],
                });
        let stroke_pp_esti_pipeline_layout =
            self.device
                .create_pipeline_layout(&PipelineLayoutDescriptor {
                    label: Some("brush stroke postprocess estimation pipeline layout"),
                    bind_group_layouts: &[&stroke_pp_esti_layout],
                    push_constant_ranges: &[],
                });

        let mut stroke_pp_pipelines = Vec::new();
        let mut stroke_pp_esti_pipelines = Vec::new();
        for compiled in compiled_preset.stroke_postprocess_graphs {
            let shader = self.device.create_shader_module(ShaderModuleDescriptor {
                label: Some("brush stroke postprocess shader"),
                source: ShaderSource::Wgsl(compiled.shader.into()),
            });
            stroke_pp_pipelines.push(self.device.create_compute_pipeline(
                &ComputePipelineDescriptor {
                    label: Some("brush stroke postprocess pipeline"),
                    layout: Some(&stroke_pp_pipeline_layout),
                    module: &shader,
                    entry_point: Some("main"),
                    compilation_options: Default::default(),
                    cache: None,
                },
            ));

            let esti_shader = self.device.create_shader_module(ShaderModuleDescriptor {
                label: Some("brush stroke postprocess estimation shader"),
                source: ShaderSource::Wgsl(compiled.size_estimation.into()),
            });
            stroke_pp_esti_pipelines.push(self.device.create_compute_pipeline(
                &ComputePipelineDescriptor {
                    label: Some("brush stroke postprocess estimation pipeline"),
                    layout: Some(&stroke_pp_esti_pipeline_layout),
                    module: &esti_shader,
                    entry_point: Some("estimate"),
                    compilation_options: Default::default(),
                    cache: None,
                },
            ));
        }

        let mut graph_input = DynamicBuffer::new(Some("graph input buffer"), BufferUsages::STORAGE);
        graph_input.push(&GraphInputUniform {
            pen_position: Vec2::ZERO,
        });
        graph_input.write_buffer(&self.device);

        self.initialized = Some(InitializedData {
            graph_input: graph_input.into_inner_buffer().unwrap(),
            referenced_textures,
            target_layer,
            intermediate_buffers,
            stroke_info: stroke_info.into_inner_buffer().unwrap(),

            main_layout,
            main_pipeline,
            main_esti_layout,
            main_esti_pipeline,
            stroke_pp_esti_layout,
            stroke_pp_esti_pipelines,
            stroke_pp_layout,
            stroke_pp_pipelines,
        });
        self.prepared = None;
    }

    pub fn prepare(&mut self, brush: &BrushPresetInstance, tiles: &GpuTileStorage) {
        let Some(InitializedData {
            target_layer,
            referenced_textures,
            intermediate_buffers,
            graph_input,
            stroke_info,
            main_layout,
            main_esti_layout,
            ..
        }) = self.initialized.as_mut()
        else {
            return;
        };

        let mut external_var_buffers = Vec::new();
        for var in brush.external_vars().all().iter() {
            let buffer = var.value.try_write_into_shader_buffer().unwrap();
            let gpu_buffer = self.device.create_buffer_init(&BufferInitDescriptor {
                label: Some("external variable buffer"),
                contents: &buffer,
                usage: BufferUsages::STORAGE,
            });
            external_var_buffers.push(gpu_buffer);
        }
        let referenced_textures = referenced_textures
            .iter()
            .map(std::convert::identity)
            .collect::<Vec<_>>();

        // Prepare external variable buffers

        let mut external_var_bindings = Vec::with_capacity(external_var_buffers.len());
        let external_var_base_binding = EXTERNAL_VARIABLE_BASE_BINDING;
        for (index, buffer) in external_var_buffers.iter().enumerate() {
            external_var_bindings.push(BindGroupEntry {
                binding: external_var_base_binding + index as u32,
                resource: buffer.as_entire_binding(),
            });
        }

        // Layer bindings
        let target_layer = tiles.get_layer_binding_or_empty(*target_layer).unwrap();

        let intermediate_textures = intermediate_buffers.textures();
        let intermediate_tile_info = intermediate_buffers.tile_info_buffer();

        let mut main_esti_bind_groups_option = [None, None];
        for i in 0..2 {
            let main_esti_bind_group_entries = {
                let mut entries = vec![
                    BindGroupEntry {
                        binding: 0,
                        resource: graph_input.as_entire_binding(),
                    },
                    BindGroupEntry {
                        binding: 1,
                        resource: stroke_info.as_entire_binding(),
                    },
                    BindGroupEntry {
                        binding: 2,
                        resource: BindingResource::TextureViewArray(&referenced_textures),
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
                        binding: 7,
                        resource: intermediate_tile_info.as_entire_binding(),
                    },
                    BindGroupEntry {
                        binding: 8,
                        resource: BindingResource::TextureView(intermediate_textures[i].as_ref()),
                    },
                    BindGroupEntry {
                        binding: 16,
                        resource: self.tile_allocation_dispatch.as_entire_binding(),
                    },
                    BindGroupEntry {
                        binding: 17,
                        resource: self.main_dispatch.as_entire_binding(),
                    },
                ];
                entries.extend(external_var_bindings.clone());
                entries
            };

            let main_esti_bind_group = self.device.create_bind_group(&BindGroupDescriptor {
                label: Some("brush main estimation bind group"),
                layout: &main_esti_layout,
                entries: &main_esti_bind_group_entries,
            });

            main_esti_bind_groups_option[i].replace(main_esti_bind_group);
        }

        let mut main_bind_groups_option = [None, None];
        for i in 0..2 {
            let mut main_bind_group_entries = vec![
                BindGroupEntry {
                    binding: 0,
                    resource: graph_input.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 1,
                    resource: stroke_info.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 2,
                    resource: BindingResource::TextureViewArray(&referenced_textures),
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
                    resource: intermediate_tile_info.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 6,
                    resource: BindingResource::TextureView(intermediate_textures[1 - i].as_ref()),
                },
                BindGroupEntry {
                    binding: 7,
                    resource: intermediate_tile_info.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 8,
                    resource: BindingResource::TextureView(intermediate_textures[i].as_ref()),
                },
            ];

            main_bind_group_entries.extend(external_var_bindings.clone());

            let main_bind_group = self.device.create_bind_group(&BindGroupDescriptor {
                label: Some("brush main bind group"),
                layout: &main_layout,
                entries: &main_bind_group_entries,
            });

            main_bind_groups_option[i].replace(main_bind_group);
        }

        let tile_allocation_bind_group = self.device.create_bind_group(&BindGroupDescriptor {
            label: Some("tile allocation bind group"),
            layout: &self.tile_allocation_layout,
            entries: &[
                BindGroupEntry {
                    binding: 0,
                    resource: stroke_info.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 1,
                    resource: intermediate_tile_info.as_entire_binding(),
                },
            ],
        });

        self.prepared = Some(PreparedData {
            external_var_buffers,
            tile_allocation_bind_group,
            main_esti_bind_groups: [
                main_esti_bind_groups_option[0].take().unwrap(),
                main_esti_bind_groups_option[1].take().unwrap(),
            ],
            main_bind_groups: [
                main_bind_groups_option[0].take().unwrap(),
                main_bind_groups_option[1].take().unwrap(),
            ],
            next_bind_group: 0,
        })
    }

    pub fn draw_main(
        &mut self,
        params: GraphInputParams,
        tiles: &GpuTileStorage,
        ec: &mut CommandEncoder,
    ) {
        let (
            Some(InitializedData {
                target_layer,
                graph_input,
                referenced_textures,
                main_layout,
                main_esti_layout,
                main_esti_pipeline,
                main_pipeline,
                intermediate_buffers,
                stroke_info,
                ..
            }),
            Some(PreparedData {
                external_var_buffers,
                main_bind_groups,
                main_esti_bind_groups,
                next_bind_group,
                tile_allocation_bind_group,
                ..
            }),
        ) = (self.initialized.as_mut(), self.prepared.as_mut())
        else {
            return;
        };

        // Prepare buffers

        let mut wrapper = StorageBuffer::new(Vec::<u8>::new());
        wrapper
            .write(&GraphInputUniform {
                pen_position: params.pen_position,
            })
            .unwrap();
        self.queue.write_buffer(graph_input, 0, wrapper.as_ref());

        ec.push_debug_group("brush main estimation");
        {
            let mut pass = ec.begin_compute_pass(&Default::default());
            pass.set_pipeline(&main_esti_pipeline);
            pass.set_bind_group(0, &main_esti_bind_groups[*next_bind_group], &[]);
            pass.dispatch_workgroups(1, 1, 1);
        }
        ec.pop_debug_group();

        // -----------------------
        // Step 2: Allocate affected tiles
        // -----------------------

        ec.push_debug_group("tile allocation");
        {
            let mut pass = ec.begin_compute_pass(&Default::default());
            pass.set_pipeline(&self.tile_allocation_pipeline);
            pass.set_bind_group(0, &*tile_allocation_bind_group, &[]);
            pass.dispatch_workgroups_indirect(&self.tile_allocation_dispatch, 0);
        }
        ec.pop_debug_group();

        // -----------------------
        // Step 3: Main pass
        // -----------------------

        // Prepare main bind group

        ec.push_debug_group("brush main");
        {
            let mut pass = ec.begin_compute_pass(&Default::default());
            pass.set_pipeline(&main_pipeline);
            pass.set_bind_group(0, &main_bind_groups[*next_bind_group], &[]);
            pass.dispatch_workgroups_indirect(&self.main_dispatch, 0);
        }
        ec.pop_debug_group();

        *next_bind_group = 1 - *next_bind_group;
        intermediate_buffers.swap();
    }

    pub fn draw_stroke_postprocess(&mut self, tiles: &GpuTileStorage) {
        let (
            Some(InitializedData {
                target_layer,
                referenced_textures,
                stroke_info,
                stroke_pp_pipelines,
                stroke_pp_layout,
                stroke_pp_esti_pipelines,
                stroke_pp_esti_layout,
                intermediate_buffers,
                ..
            }),
            Some(PreparedData {
                external_var_buffers,
                ..
            }),
        ) = (self.initialized.as_mut(), self.prepared.as_ref())
        else {
            return;
        };

        let target_layer_tiles = tiles.get_layer_binding_or_empty(*target_layer).unwrap();

        let referenced_texture_views = referenced_textures
            .iter()
            .map(std::convert::identity)
            .collect::<Vec<_>>();

        let mut bind_group_entries_base = vec![
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

        let total_stroke_pp = stroke_pp_pipelines.len();

        // Prepare external variable buffers

        let mut external_var_bindings = Vec::with_capacity(external_var_buffers.len());
        let external_var_base_binding = EXTERNAL_VARIABLE_BASE_BINDING;
        for (index, buffer) in external_var_buffers.iter().enumerate() {
            external_var_bindings.push(BindGroupEntry {
                binding: external_var_base_binding + index as u32,
                resource: buffer.as_entire_binding(),
            });
        }
        bind_group_entries_base.extend(external_var_bindings);

        let mut ec = self.device.create_command_encoder(&Default::default());

        for pp_index in 0..total_stroke_pp {
            let src_tex = intermediate_buffers.src_tex();
            let dst_tex = intermediate_buffers.dst_tex();
            let tile_info_buf = intermediate_buffers.tile_info_buffer();
            intermediate_buffers.swap();

            // -----------------------
            // Step 1: Estimate affected tiles
            // -----------------------

            let esti_entries = {
                let mut entries = bind_group_entries_base.clone();
                entries.extend([
                    BindGroupEntry {
                        binding: 1,
                        resource: stroke_info.as_entire_binding(),
                    },
                    BindGroupEntry {
                        binding: 7,
                        resource: tile_info_buf.as_entire_binding(),
                    },
                    BindGroupEntry {
                        binding: 8,
                        resource: BindingResource::TextureView(src_tex.as_ref()),
                    },
                    BindGroupEntry {
                        binding: 16,
                        resource: self.tile_allocation_dispatch.as_entire_binding(),
                    },
                    BindGroupEntry {
                        binding: 17,
                        resource: self.main_dispatch.as_entire_binding(),
                    },
                ]);
                entries
            };

            let esti_bind_group = self.device.create_bind_group(&BindGroupDescriptor {
                label: Some("brush main estimation bind group"),
                layout: &stroke_pp_esti_layout,
                entries: &esti_entries,
            });

            ec.push_debug_group(&format!("brush stroke postprocess estimation {}", pp_index));
            {
                let mut pass = ec.begin_compute_pass(&Default::default());
                pass.set_pipeline(&stroke_pp_esti_pipelines[pp_index]);
                pass.set_bind_group(0, &esti_bind_group, &[]);
                pass.dispatch_workgroups(1, 1, 1);
            }
            ec.pop_debug_group();

            // -----------------------
            // Step 2: Allocate affected tiles
            // -----------------------

            let tile_allocation_bind_group = self.device.create_bind_group(&BindGroupDescriptor {
                label: Some("tile allocation bind group"),
                layout: &self.tile_allocation_layout,
                entries: &[
                    BindGroupEntry {
                        binding: 0,
                        resource: stroke_info.as_entire_binding(),
                    },
                    BindGroupEntry {
                        binding: 1,
                        resource: tile_info_buf.as_entire_binding(),
                    },
                ],
            });

            ec.push_debug_group(&format!(
                "brush stroke postprocess tile allocation {}",
                pp_index
            ));
            {
                let mut pass = ec.begin_compute_pass(&Default::default());
                pass.set_pipeline(&self.tile_allocation_pipeline);
                pass.set_bind_group(0, &tile_allocation_bind_group, &[]);
                pass.dispatch_workgroups_indirect(&self.tile_allocation_dispatch, 0);
            }
            ec.pop_debug_group();

            // -----------------------
            // Step 3: Main pass
            // -----------------------

            let entries = {
                let mut entries = bind_group_entries_base.clone();
                entries.extend([
                    BindGroupEntry {
                        binding: 1,
                        resource: stroke_info.as_entire_binding(),
                    },
                    BindGroupEntry {
                        binding: 5,
                        resource: tile_info_buf.as_entire_binding(),
                    },
                    BindGroupEntry {
                        binding: 6,
                        resource: BindingResource::TextureView(dst_tex.as_ref()),
                    },
                    BindGroupEntry {
                        binding: 7,
                        resource: tile_info_buf.as_entire_binding(),
                    },
                    BindGroupEntry {
                        binding: 8,
                        resource: BindingResource::TextureView(src_tex.as_ref()),
                    },
                ]);
                entries
            };
            let bind_group = self.device.create_bind_group(&BindGroupDescriptor {
                label: Some("brush stroke postprocess bind group"),
                layout: &stroke_pp_layout,
                entries: &entries,
            });

            ec.push_debug_group(&format!("brush stroke postprocess {}", pp_index));
            {
                let mut pass = ec.begin_compute_pass(&Default::default());
                pass.set_pipeline(&stroke_pp_pipelines[pp_index]);
                pass.set_bind_group(0, &bind_group, &[]);
                pass.dispatch_workgroups_indirect(&self.main_dispatch, 0);
            }
            ec.pop_debug_group();
        }

        self.queue.submit([ec.finish()]);
    }

    pub fn copy_last_surface_to_target(&self, tiles: &GpuTileStorage) {
        let Some(InitializedData {
            target_layer,
            intermediate_buffers,
            ..
        }) = self.initialized.as_ref()
        else {
            return;
        };

        let tile_info = intermediate_buffers.tile_info_buffer();

        let tile_info_staging = self.device.create_buffer(&BufferDescriptor {
            label: Some("tile info staging"),
            size: tile_info.size(),
            usage: BufferUsages::COPY_DST | BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });

        let mut ec = self.device.create_command_encoder(&Default::default());
        ec.copy_buffer_to_buffer(&tile_info, 0, &tile_info_staging, 0, tile_info.size());
        let submission_index = self.queue.submit([ec.finish()]);

        let (tx, rx) = std::sync::mpsc::channel();
        tile_info_staging
            .slice(..)
            .map_async(MapMode::Read, move |r| tx.send(r).unwrap());
        self.device
            .poll(PollType::Wait {
                submission_index: Some(submission_index),
                timeout: None,
            })
            .unwrap();
        rx.recv().unwrap().unwrap();

        let tile_info_data = tile_info_staging.slice(..).get_mapped_range();
        let tile_info = {
            let storage = encase::StorageBuffer::new(tile_info_data.as_ref());
            storage.create::<DynamicGpuTileInfoBuffer>().unwrap()
        };
        drop(tile_info_data);

        let mut target_layer = tiles.get_layer_mut(*target_layer).unwrap();

        for tile in &tile_info.buf {
            if tile.index == IVec2::MIN {
                break;
            }
            target_layer.get_tile_or_allocate(tile.index);
        }

        let result_layer = intermediate_buffers.src_tex();
        let mut ec = self.device.create_command_encoder(&Default::default());
        for (src, tile) in tile_info.buf.iter().enumerate() {
            if tile.index == IVec2::MIN {
                break;
            }

            let dst = target_layer.get_tile_layer(tile.index).unwrap();

            ec.copy_texture_to_texture(
                TexelCopyTextureInfo {
                    texture: result_layer.texture(),
                    mip_level: 0,
                    origin: Origin3d {
                        x: 0,
                        y: 0,
                        z: src as u32,
                    },
                    aspect: TextureAspect::All,
                },
                TexelCopyTextureInfo {
                    texture: target_layer.texture().unwrap().texture(),
                    mip_level: 0,
                    origin: Origin3d { x: 0, y: 0, z: dst },
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
}
