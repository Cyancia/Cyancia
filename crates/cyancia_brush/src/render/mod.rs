use std::{
    collections::{HashSet, VecDeque},
    num::NonZeroU32,
    ops::Deref,
    sync::Arc,
};

use bevy_math::IRect;
use bytemuck::Contiguous;
use cyancia_assets::{asset::AssetId, store::AssetRegistry};
use cyancia_image::{
    layer::LayerId,
    texel::{TexelDepth, TexelFormat, TexelType},
    tile::{
        DynamicLayerStorage, GpuLayerInfo, GpuTileInfo, GpuTileStorage, GpuTileStorageInner, Tile,
        TileIndex,
    },
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
    BindGroupLayoutEntry, BindingResource, BindingType, Buffer, BufferAddress, BufferBindingType,
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
        pipelines::{
            BrushEstimatePipeline, BrushInputSamplingPipeline, BrushMainPipeline,
            BrushTileAllocationPipeline,
        },
    },
};

pub mod dynamic_intermediate_buffer;
pub mod graph;
pub mod pipelines;

const EXTERNAL_VARIABLE_BASE_BINDING: u32 = 32;

pub struct BrushPresetOperator {
    instance: Arc<RwLock<BrushPresetInstance>>,
    device: Device,
    queue: Queue,
    renderer: Option<BrushPresetRenderer>,
}

impl BrushPresetOperator {
    pub fn new(instance: Arc<RwLock<BrushPresetInstance>>, device: Device, queue: Queue) -> Self {
        Self {
            instance,
            renderer: None,
            device,
            queue,
        }
    }

    pub fn begin_stroke(
        &mut self,
        input: PenInput,
        tiles: &GpuTileStorage,
        assets: &AssetRegistry,
        target_layer: LayerId,
    ) {
        let instance = self.instance.read();
        let now = std::time::Instant::now();
        let renderer = BrushPresetRenderer::new(
            &self.device,
            &self.queue,
            &instance,
            tiles,
            target_layer,
            assets,
        );
        log::info!("Brush preset renderer creation: {:?}", now.elapsed());
        renderer.update(&self.device, &self.queue, input);
        self.renderer = Some(renderer);
    }

    pub fn update_stroke(&mut self, input: PenInput) {
        if let Some(renderer) = &self.renderer {
            renderer.update(&self.device, &self.queue, input);
        }
    }

    pub fn end_stroke(&mut self, tiles: &GpuTileStorage, target_layer: LayerId) {
        let Some(renderer) = &self.renderer else {
            return;
        };

        let now = std::time::Instant::now();
        renderer.copy_last_surface_to_layer(&self.device, &self.queue, tiles, target_layer);
        log::info!("Brush stroke postprocess and copy: {:?}", now.elapsed());
    }
}

pub struct BrushPresetRenderer {
    input_sampling: BrushInputSamplingPipeline,
    tile_allocation: BrushTileAllocationPipeline,
    estimate: BrushEstimatePipeline,
    main: BrushMainPipeline,
    stroke_pp_estimate: BrushEstimatePipeline,
    stroke_pp_main: BrushMainPipeline,
    resources: StrokeResources,
}

impl BrushPresetRenderer {
    pub fn new(
        device: &Device,
        queue: &Queue,
        brush: &BrushPresetInstance,
        tiles: &GpuTileStorage,
        target_layer_id: LayerId,
        assets: &AssetRegistry,
    ) -> Self {
        let compiled_brush = brush.compile(EXTERNAL_VARIABLE_BASE_BINDING).unwrap();
        let resources = StrokeResources::new(
            device,
            queue,
            &compiled_brush,
            target_layer_id,
            tiles,
            assets,
        );

        let input_sampling = BrushInputSamplingPipeline::new(
            device,
            &resources,
            compiled_brush.input_sampling.into(),
        );
        let tile_allocation = BrushTileAllocationPipeline::new(device, &resources);
        let estimate = BrushEstimatePipeline::new(
            device,
            &resources,
            compiled_brush.main_graph.size_estimation.into(),
        );
        let main =
            BrushMainPipeline::new(device, &resources, compiled_brush.main_graph.main.into());
        let stroke_pp_estimate = BrushEstimatePipeline::new(
            device,
            &resources,
            compiled_brush
                .stroke_postprocess_graphs
                .size_estimation
                .into(),
        );
        let stroke_pp_main = BrushMainPipeline::new(
            device,
            &resources,
            compiled_brush.stroke_postprocess_graphs.main.into(),
        );

        Self {
            input_sampling,
            tile_allocation,
            estimate,
            main,
            stroke_pp_estimate,
            stroke_pp_main,
            resources,
        }
    }

    pub fn update(&self, device: &Device, queue: &Queue, input: PenInput) {
        let mut input_staging =
            DynamicBuffer::new(Some("pen input staging buffer"), BufferUsages::COPY_SRC);
        input_staging.push(&input);
        input_staging.write_buffer(device);

        let mut ec = device.create_command_encoder(&Default::default());

        ec.copy_buffer_to_buffer(
            &input_staging.into_inner_buffer().unwrap(),
            0,
            &self.resources.pen_input,
            0,
            PenInput::min_size().into_integer(),
        );
        ec.clear_buffer(&self.resources.pass_fence, 0, None);

        {
            ec.push_debug_group("brush preset update stroke");
            self.input_sampling.dispatch(&mut ec);
            self.estimate.dispatch_indirect(&mut ec, &self.resources);
            self.tile_allocation.dispatch(&mut ec, &self.resources);
            self.main.dispatch(&mut ec, &self.resources);
            ec.pop_debug_group();
        }

        queue.submit([ec.finish()]);
    }

    pub fn copy_last_surface_to_layer(
        &self,
        device: &Device,
        queue: &Queue,
        tiles: &GpuTileStorage,
        target_layer_id: LayerId,
    ) {
        let tile_info = self.resources.intermediate_buffers.tile_info_buffer();
        let tile_info_staging = device.create_buffer(&BufferDescriptor {
            label: Some("tile info staging"),
            size: tile_info.size(),
            usage: BufferUsages::COPY_DST | BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let stroke_info = &self.resources.stroke_info;
        let stroke_info_staging = device.create_buffer(&BufferDescriptor {
            label: Some("stroke info staging"),
            size: stroke_info.size(),
            usage: BufferUsages::COPY_DST | BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });

        let mut ec = device.create_command_encoder(&Default::default());
        ec.copy_buffer_to_buffer(&tile_info, 0, &tile_info_staging, 0, tile_info.size());
        ec.copy_buffer_to_buffer(stroke_info, 0, &stroke_info_staging, 0, stroke_info.size());
        let submission_index = queue.submit([ec.finish()]);

        let (tx, rx) = std::sync::mpsc::channel();
        {
            let tx = tx.clone();
            tile_info_staging
                .slice(..)
                .map_async(MapMode::Read, move |r| tx.send(r).unwrap());
        }
        {
            let tx = tx.clone();
            stroke_info_staging
                .slice(..)
                .map_async(MapMode::Read, move |r| tx.send(r).unwrap());
        }
        device
            .poll(PollType::Wait {
                submission_index: Some(submission_index),
                timeout: None,
            })
            .unwrap();

        rx.recv().unwrap().unwrap();
        rx.recv().unwrap().unwrap();

        let tile_info = {
            let tile_info_data = tile_info_staging.slice(..).get_mapped_range();
            let storage = encase::StorageBuffer::new(tile_info_data.as_ref());
            storage.create::<DynamicGpuTileInfoBuffer>().unwrap()
        };
        let stroke_info = {
            let stroke_info_data = stroke_info_staging.slice(..).get_mapped_range();
            let storage = encase::StorageBuffer::new(stroke_info_data.as_ref());
            storage.create::<StrokeInfo>().unwrap()
        };

        let mut target_layer = tiles.get_layer_mut(target_layer_id).unwrap();

        for tile in &tile_info.buf {
            if tile.index == IVec2::MIN {
                break;
            }

            target_layer.get_tile_or_allocate(tile.index);
        }

        let result_layer = if (stroke_info.total_dabs + self.resources.n_stroke_pp) % 2 == 0 {
            &self.resources.intermediate_buffers.textures()[1]
        } else {
            &self.resources.intermediate_buffers.textures()[0]
        };

        let mut ec = device.create_command_encoder(&Default::default());
        ec.push_debug_group("copy brush preset result to target layer");
        let mut n_copied = 0;
        for (src, tile) in tile_info.buf.iter().enumerate() {
            if tile.index == IVec2::MIN {
                break;
            }
            n_copied += 1;

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
        ec.pop_debug_group();
        queue.submit([ec.finish()]);

        log::info!("Copied {} tiles to target layer", n_copied);
    }
}

pub const MAX_SAMPLES_BETWEEN_INPUTS: usize = 256;

#[derive(ShaderType, Default, Clone, Copy)]
pub struct ComputedPenInput {
    pub position: Vec2,
}

#[derive(ShaderType, Default, Clone, Copy)]
pub struct PenInput {
    pub position: Vec2,
}

#[derive(ShaderType, Default, Clone, Copy)]
pub struct StrokeInfo {
    pub accumulated_bound_min: IVec2,
    pub accumulated_bound_max: IVec2,
    pub max_affected_tiles_count: UVec2,
    pub total_dabs: u32,
    pub _padding: u32,
}

#[derive(ShaderType, Clone, Copy)]
pub struct OutputSamples {
    pub n_samples: u32,
    pub samples: [ComputedPenInput; MAX_SAMPLES_BETWEEN_INPUTS],
}

impl Default for OutputSamples {
    fn default() -> Self {
        Self {
            n_samples: 0,
            samples: [ComputedPenInput::default(); MAX_SAMPLES_BETWEEN_INPUTS],
        }
    }
}

#[derive(ShaderType, Default, Clone, Copy)]
pub struct DabInfo {
    pub bound_min: IVec2,
    pub bound_max: IVec2,
}

#[derive(ShaderType, Clone, Copy)]
pub struct DabInfos {
    pub n_dabs: u32,
    pub buf: [DabInfo; MAX_SAMPLES_BETWEEN_INPUTS],
}

impl Default for DabInfos {
    fn default() -> Self {
        Self {
            n_dabs: 0,
            buf: [DabInfo::default(); MAX_SAMPLES_BETWEEN_INPUTS],
        }
    }
}

#[derive(ShaderType, Default, Clone, Copy)]
pub struct PassFence {
    pub cur_sample: u32,
    pub cur_sample_finished_threads: u32,
}

#[derive(ShaderType, Default, Clone, Copy)]
pub struct PenInputSampler {
    pub last_input: PenInput,
    pub last_sample: ComputedPenInput,
    pub has_last_sample: u32,
}

pub struct StrokeResources {
    pub n_stroke_pp: u32,

    pub pen_input: Buffer,
    pub input_sampler: Buffer,
    pub output_samples: Buffer,
    pub stroke_info: Buffer,
    pub dab_infos: Buffer,
    pub pass_fence: Buffer,

    pub external_var_layouts: Vec<BindGroupLayoutEntry>,
    pub external_var_buffers: Vec<Buffer>,
    pub referenced_textures: Vec<TextureView>,

    pub intermediate_buffers: DynamicIntermediateBuffer,
    pub target_layer_id: LayerId,
    pub target_layer: TextureView,
    pub target_layer_tile_info: Buffer,

    pub estimate_dispatch: Buffer,
    pub tile_allocation_dispatch: Buffer,
    pub main_dispatch: Buffer,
}

impl StrokeResources {
    pub fn new(
        device: &Device,
        queue: &Queue,
        brush: &CompiledBrushPreset,
        target_layer_id: LayerId,
        tiles: &GpuTileStorage,
        assets: &AssetRegistry,
    ) -> Self {
        let mut pen_input = DynamicBuffer::new(Some("pen input buffer"), BufferUsages::STORAGE);
        pen_input.push(&PenInput::default());
        pen_input.write_buffer(device);

        let mut input_sampler =
            DynamicBuffer::new(Some("pen input sampler buffer"), BufferUsages::STORAGE);
        input_sampler.push(&PenInputSampler::default());
        input_sampler.write_buffer(device);

        let mut output_samples =
            DynamicBuffer::new(Some("output samples buffer"), BufferUsages::STORAGE);
        output_samples.push(&OutputSamples::default());
        output_samples.write_buffer(device);

        let mut stroke_info = DynamicBuffer::new(
            Some("stroke info buffer"),
            BufferUsages::STORAGE | BufferUsages::COPY_SRC,
        );
        stroke_info.push(&StrokeInfo::default());
        stroke_info.write_buffer(device);

        let mut dab_infos = DynamicBuffer::new(Some("dab infos buffer"), BufferUsages::STORAGE);
        dab_infos.push(&DabInfos::default());
        dab_infos.write_buffer(device);

        let mut main_pass_sync =
            DynamicBuffer::new(Some("main pass sync buffer"), BufferUsages::STORAGE);
        main_pass_sync.push(&PassFence::default());
        main_pass_sync.write_buffer(device);

        let mut external_var_layouts = Vec::new();
        let mut cur_binding = EXTERNAL_VARIABLE_BASE_BINDING;
        for _ in 0..brush.external_vars.all().len() {
            external_var_layouts.push(BindGroupLayoutEntry {
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

        let mut external_var_buffers = Vec::new();
        for var in brush.external_vars.all().iter() {
            let buffer = var.value.try_write_into_shader_buffer().unwrap();
            let gpu_buffer = device.create_buffer_init(&BufferInitDescriptor {
                label: Some("external variable buffer"),
                contents: &buffer,
                usage: BufferUsages::STORAGE,
            });
            external_var_buffers.push(gpu_buffer);
        }

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
            format: TextureFormat::Rgba8Unorm,
            usage: TextureUsages::TEXTURE_BINDING | TextureUsages::STORAGE_BINDING,
            view_formats: &[],
        });
        let mut referenced_textures = Vec::new();
        for id in &brush.texture_usage {
            if *id == TextureId::NULL {
                referenced_textures.push(empty_texture.create_view(&Default::default()));
                continue;
            }

            let handle = assets.handle(AssetId::new(**id)).unwrap();
            let gpu_image = GpuImage::from_asset(
                &device,
                &queue,
                &handle.get().unwrap(),
                TextureUsages::TEXTURE_BINDING,
            );
            referenced_textures.push(gpu_image.texture.create_view(&Default::default()));
        }

        let target_layer_binding = tiles.get_layer_binding_or_empty(target_layer_id).unwrap();
        let target_layer_info = tiles.get_layer_info(target_layer_id).unwrap();

        let intermediate_buffers =
            DynamicIntermediateBuffer::new(256, target_layer_info.texel_type, device.clone());

        let mut estimate_dispatch = DynamicBuffer::new(
            Some("estimate dispatch buffer"),
            BufferUsages::STORAGE | BufferUsages::INDIRECT,
        );
        estimate_dispatch.push(&UVec4::ZERO);
        estimate_dispatch.write_buffer(device);

        let mut tile_allocation_dispatch = DynamicBuffer::new(
            Some("tile allocation dispatch buffer"),
            BufferUsages::STORAGE | BufferUsages::INDIRECT,
        );
        tile_allocation_dispatch.push(&UVec4::ZERO);
        tile_allocation_dispatch.write_buffer(device);

        let mut main_dispatch = DynamicBuffer::new(
            Some("main dispatch buffer"),
            BufferUsages::STORAGE | BufferUsages::INDIRECT,
        );
        main_dispatch.push(&UVec4::ZERO);
        main_dispatch.write_buffer(device);

        Self {
            n_stroke_pp: brush.n_stroke_postprocess_graphs,

            pen_input: pen_input.into_inner_buffer().unwrap(),
            input_sampler: input_sampler.into_inner_buffer().unwrap(),
            output_samples: output_samples.into_inner_buffer().unwrap(),
            stroke_info: stroke_info.into_inner_buffer().unwrap(),
            dab_infos: dab_infos.into_inner_buffer().unwrap(),
            pass_fence: main_pass_sync.into_inner_buffer().unwrap(),

            external_var_layouts,
            external_var_buffers,
            referenced_textures,

            intermediate_buffers,
            target_layer_id,
            target_layer: target_layer_binding.texture.deref().clone(),
            target_layer_tile_info: target_layer_binding.tile_info_buffer,

            estimate_dispatch: estimate_dispatch.into_inner_buffer().unwrap(),
            tile_allocation_dispatch: tile_allocation_dispatch.into_inner_buffer().unwrap(),
            main_dispatch: main_dispatch.into_inner_buffer().unwrap(),
        }
    }

    pub fn external_var_bindings(&self) -> Vec<BindGroupEntry<'_>> {
        self.external_var_buffers
            .iter()
            .enumerate()
            .map(|(i, buffer)| BindGroupEntry {
                binding: EXTERNAL_VARIABLE_BASE_BINDING + i as u32,
                resource: BindingResource::Buffer(buffer.as_entire_buffer_binding()),
            })
            .collect()
    }
}
