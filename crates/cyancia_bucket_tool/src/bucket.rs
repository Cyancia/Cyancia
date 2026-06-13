use std::{
    collections::HashSet,
    sync::{Arc, mpsc},
};

use cyancia_image::{
    texel::TexelType,
    tile::{DynamicLayerStorage, GpuTileInfo, GpuTileStorageInner, LayerBindingData, TileIndex},
};
use cyancia_render::buffer::{BufferVec, DynamicBuffer};
use encase::ShaderType;
use glam::{IVec2, UVec2, Vec4};
use indexmap::IndexMap;
use wesl::include_wesl;
use wgpu::{
    BindGroupDescriptor, BindGroupEntry, BindGroupLayout, BindGroupLayoutDescriptor,
    BindGroupLayoutEntry, BindingResource, BindingType, Buffer, BufferBindingType, BufferUsages,
    ComputePassDescriptor, ComputePipeline, ComputePipelineDescriptor, Device, Extent3d, MapMode,
    PipelineLayoutDescriptor, PollType, Queue, ShaderModuleDescriptor, ShaderSource, ShaderStages,
    StorageTextureAccess, TextureDimension, TextureFormat, TextureUsages, TextureViewDimension,
    wgt::{BufferDescriptor, TextureDescriptor, TextureViewDescriptor},
};

// TODO Use 16 bit/8 bit if possible
pub const MASK_TEXTURE_FORMAT: TextureFormat = TextureFormat::R32Float;

#[derive(ShaderType, Debug, Clone, Copy)]
pub struct BucketParams {
    pub seed: UVec2,
    pub fill_color: Vec4,
    pub threshold: f32,
    pub alpha_threshold: f32,
    pub grow: i32,
}

#[derive(ShaderType, Debug, Clone, Copy)]
pub struct SmaaParams {
    pub blend_strength: f32,
    pub diagonal_weight: f32,
    pub corner_preserve_strength: f32,
    pub edge_search_steps: u32,
}

impl Default for SmaaParams {
    fn default() -> Self {
        Self {
            blend_strength: 0.45,
            diagonal_weight: 0.5,
            corner_preserve_strength: 0.75,
            edge_search_steps: 4,
        }
    }
}

pub struct Bucket {
    thresholding_layout: BindGroupLayout,
    thresholding_pipeline: ComputePipeline,
    ccl_layout: BindGroupLayout,
    ccl_merge_pipeline: ComputePipeline,
    ccl_compress_pipeline: ComputePipeline,
    ccl_extract_pipeline: ComputePipeline,
    grow_estimate_layout: BindGroupLayout,
    grow_estimate_pipeline: ComputePipeline,
    grow_main_layout: BindGroupLayout,
    grow_main_pipeline: ComputePipeline,
    smaa_layout: BindGroupLayout,
    smaa_pipeline: ComputePipeline,
    composite_layout: BindGroupLayout,
    composite_pipeline: ComputePipeline,
}

impl Bucket {
    pub fn new(device: &Device, ref_texel_type: TexelType, output_texel_type: TexelType) -> Self {
        let thresholding_layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some("thresholding_layout"),
            entries: &[
                BindGroupLayoutEntry {
                    binding: 0,
                    visibility: ShaderStages::COMPUTE,
                    ty: BindingType::Buffer {
                        ty: BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: Some(BucketParams::min_size()),
                    },
                    count: None,
                },
                BindGroupLayoutEntry {
                    binding: 1,
                    visibility: ShaderStages::COMPUTE,
                    ty: BindingType::StorageTexture {
                        access: StorageTextureAccess::ReadOnly,
                        format: ref_texel_type.wgpu_format(),
                        view_dimension: TextureViewDimension::D2Array,
                    },
                    count: None,
                },
                BindGroupLayoutEntry {
                    binding: 2,
                    visibility: ShaderStages::COMPUTE,
                    ty: BindingType::Buffer {
                        ty: BufferBindingType::Storage { read_only: false },
                        has_dynamic_offset: false,
                        min_binding_size: Some(u32::min_size()),
                    },
                    count: None,
                },
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
            ],
        });

        let thresholding_pipeline_layout =
            device.create_pipeline_layout(&PipelineLayoutDescriptor {
                label: Some("thresholding_pipeline_layout"),
                bind_group_layouts: &[&thresholding_layout],
                push_constant_ranges: &[],
            });
        let thresholding_shader = device.create_shader_module(ShaderModuleDescriptor {
            label: Some("thresholding_shader_module"),
            source: ShaderSource::Wgsl(include_wesl!("thresholding").into()),
        });
        let thresholding_pipeline = device.create_compute_pipeline(&ComputePipelineDescriptor {
            label: Some("thresholding_pipeline"),
            layout: Some(&thresholding_pipeline_layout),
            module: &thresholding_shader,
            entry_point: Some("main"),
            compilation_options: Default::default(),
            cache: None,
        });

        let ccl_layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some("ccl_layout"),
            entries: &[
                BindGroupLayoutEntry {
                    binding: 0,
                    visibility: ShaderStages::COMPUTE,
                    ty: BindingType::Buffer {
                        ty: BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: Some(BucketParams::min_size()),
                    },
                    count: None,
                },
                BindGroupLayoutEntry {
                    binding: 1,
                    visibility: ShaderStages::COMPUTE,
                    ty: BindingType::StorageTexture {
                        access: StorageTextureAccess::WriteOnly,
                        format: MASK_TEXTURE_FORMAT,
                        view_dimension: TextureViewDimension::D2Array,
                    },
                    count: None,
                },
                BindGroupLayoutEntry {
                    binding: 2,
                    visibility: ShaderStages::COMPUTE,
                    ty: BindingType::Buffer {
                        ty: BufferBindingType::Storage { read_only: false },
                        has_dynamic_offset: false,
                        min_binding_size: Some(u32::min_size()),
                    },
                    count: None,
                },
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
            ],
        });

        let ccl_pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: Some("ccl_pipeline_layout"),
            bind_group_layouts: &[&ccl_layout],
            push_constant_ranges: &[],
        });

        let ccl_shader = device.create_shader_module(ShaderModuleDescriptor {
            label: Some("ccl_shader"),
            source: ShaderSource::Wgsl(include_wesl!("ccl").into()),
        });

        let ccl_merge_pipeline = device.create_compute_pipeline(&ComputePipelineDescriptor {
            label: Some("ccl_merge_pipeline"),
            layout: Some(&ccl_pipeline_layout),
            module: &ccl_shader,
            entry_point: Some("ccl_merge"),
            compilation_options: Default::default(),
            cache: None,
        });

        let ccl_compress_pipeline = device.create_compute_pipeline(&ComputePipelineDescriptor {
            label: Some("ccl_compress_pipeline"),
            layout: Some(&ccl_pipeline_layout),
            module: &ccl_shader,
            entry_point: Some("ccl_compress"),
            compilation_options: Default::default(),
            cache: None,
        });

        let ccl_extract_pipeline = device.create_compute_pipeline(&ComputePipelineDescriptor {
            label: Some("ccl_extract_pipeline"),
            layout: Some(&ccl_pipeline_layout),
            module: &ccl_shader,
            entry_point: Some("ccl_extract"),
            compilation_options: Default::default(),
            cache: None,
        });

        let grow_estimate_layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some("grow_estimate_layout"),
            entries: &[
                BindGroupLayoutEntry {
                    binding: 0,
                    visibility: ShaderStages::COMPUTE,
                    ty: BindingType::Buffer {
                        ty: BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: Some(BucketParams::min_size()),
                    },
                    count: None,
                },
                BindGroupLayoutEntry {
                    binding: 1,
                    visibility: ShaderStages::COMPUTE,
                    ty: BindingType::StorageTexture {
                        access: StorageTextureAccess::ReadOnly,
                        format: MASK_TEXTURE_FORMAT,
                        view_dimension: TextureViewDimension::D2Array,
                    },
                    count: None,
                },
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
                BindGroupLayoutEntry {
                    binding: 5,
                    visibility: ShaderStages::COMPUTE,
                    ty: BindingType::Buffer {
                        ty: BufferBindingType::Storage { read_only: false },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });

        let grow_main_layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some("grow_main_layout"),
            entries: &[
                BindGroupLayoutEntry {
                    binding: 0,
                    visibility: ShaderStages::COMPUTE,
                    ty: BindingType::Buffer {
                        ty: BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: Some(BucketParams::min_size()),
                    },
                    count: None,
                },
                BindGroupLayoutEntry {
                    binding: 1,
                    visibility: ShaderStages::COMPUTE,
                    ty: BindingType::StorageTexture {
                        access: StorageTextureAccess::ReadOnly,
                        format: MASK_TEXTURE_FORMAT,
                        view_dimension: TextureViewDimension::D2Array,
                    },
                    count: None,
                },
                BindGroupLayoutEntry {
                    binding: 2,
                    visibility: ShaderStages::COMPUTE,
                    ty: BindingType::StorageTexture {
                        access: StorageTextureAccess::WriteOnly,
                        format: MASK_TEXTURE_FORMAT,
                        view_dimension: TextureViewDimension::D2Array,
                    },
                    count: None,
                },
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
                BindGroupLayoutEntry {
                    binding: 4,
                    visibility: ShaderStages::COMPUTE,
                    ty: BindingType::Buffer {
                        ty: BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: Some(GpuTileInfo::min_size()),
                    },
                    count: None,
                },
            ],
        });

        let grow_estimate_pipeline_layout =
            device.create_pipeline_layout(&PipelineLayoutDescriptor {
                label: Some("grow_estimate_pipeline_layout"),
                bind_group_layouts: &[&grow_estimate_layout],
                push_constant_ranges: &[],
            });

        let grow_main_pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: Some("grow_main_pipeline_layout"),
            bind_group_layouts: &[&grow_main_layout],
            push_constant_ranges: &[],
        });

        let grow_shader = device.create_shader_module(ShaderModuleDescriptor {
            label: Some("grow_shader"),
            source: ShaderSource::Wgsl(include_wesl!("grow").into()),
        });

        let grow_estimate_pipeline = device.create_compute_pipeline(&ComputePipelineDescriptor {
            label: Some("grow_estimate_pipeline"),
            layout: Some(&grow_estimate_pipeline_layout),
            module: &grow_shader,
            entry_point: Some("estimate"),
            compilation_options: Default::default(),
            cache: None,
        });

        let grow_main_pipeline = device.create_compute_pipeline(&ComputePipelineDescriptor {
            label: Some("grow_main_pipeline"),
            layout: Some(&grow_main_pipeline_layout),
            module: &grow_shader,
            entry_point: Some("main"),
            compilation_options: Default::default(),
            cache: None,
        });

        let smaa_layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some("smaa_layout"),
            entries: &[
                BindGroupLayoutEntry {
                    binding: 0,
                    visibility: ShaderStages::COMPUTE,
                    ty: BindingType::StorageTexture {
                        access: StorageTextureAccess::ReadOnly,
                        format: MASK_TEXTURE_FORMAT,
                        view_dimension: TextureViewDimension::D2Array,
                    },
                    count: None,
                },
                BindGroupLayoutEntry {
                    binding: 1,
                    visibility: ShaderStages::COMPUTE,
                    ty: BindingType::StorageTexture {
                        access: StorageTextureAccess::WriteOnly,
                        format: MASK_TEXTURE_FORMAT,
                        view_dimension: TextureViewDimension::D2Array,
                    },
                    count: None,
                },
                BindGroupLayoutEntry {
                    binding: 2,
                    visibility: ShaderStages::COMPUTE,
                    ty: BindingType::Buffer {
                        ty: BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: Some(GpuTileInfo::min_size()),
                    },
                    count: None,
                },
                BindGroupLayoutEntry {
                    binding: 3,
                    visibility: ShaderStages::COMPUTE,
                    ty: BindingType::Buffer {
                        ty: BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: Some(SmaaParams::min_size()),
                    },
                    count: None,
                },
            ],
        });

        let smaa_pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: Some("smaa_pipeline_layout"),
            bind_group_layouts: &[&smaa_layout],
            push_constant_ranges: &[],
        });
        let smaa_shader = device.create_shader_module(ShaderModuleDescriptor {
            label: Some("smaa_shader"),
            source: ShaderSource::Wgsl(include_wesl!("smaa").into()),
        });
        let smaa_pipeline = device.create_compute_pipeline(&ComputePipelineDescriptor {
            label: Some("smaa_pipeline"),
            layout: Some(&smaa_pipeline_layout),
            module: &smaa_shader,
            entry_point: Some("main"),
            compilation_options: Default::default(),
            cache: None,
        });

        let composite_layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some("composite_layout"),
            entries: &[
                BindGroupLayoutEntry {
                    binding: 0,
                    visibility: ShaderStages::COMPUTE,
                    ty: BindingType::Buffer {
                        ty: BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: Some(BucketParams::min_size()),
                    },
                    count: None,
                },
                BindGroupLayoutEntry {
                    binding: 1,
                    visibility: ShaderStages::COMPUTE,
                    ty: BindingType::StorageTexture {
                        access: StorageTextureAccess::ReadOnly,
                        format: MASK_TEXTURE_FORMAT,
                        view_dimension: TextureViewDimension::D2Array,
                    },
                    count: None,
                },
                BindGroupLayoutEntry {
                    binding: 2,
                    visibility: ShaderStages::COMPUTE,
                    ty: BindingType::Buffer {
                        ty: BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: Some(GpuTileInfo::min_size()),
                    },
                    count: None,
                },
                BindGroupLayoutEntry {
                    binding: 3,
                    visibility: ShaderStages::COMPUTE,
                    ty: BindingType::StorageTexture {
                        access: StorageTextureAccess::ReadWrite,
                        format: output_texel_type.wgpu_format(),
                        view_dimension: TextureViewDimension::D2Array,
                    },
                    count: None,
                },
                BindGroupLayoutEntry {
                    binding: 4,
                    visibility: ShaderStages::COMPUTE,
                    ty: BindingType::Buffer {
                        ty: BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: Some(GpuTileInfo::min_size()),
                    },
                    count: None,
                },
            ],
        });

        let composite_pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: Some("composite_pipeline_layout"),
            bind_group_layouts: &[&composite_layout],
            push_constant_ranges: &[],
        });
        let composite_shader = device.create_shader_module(ShaderModuleDescriptor {
            label: Some("composite_shader"),
            source: ShaderSource::Wgsl(include_wesl!("composite").into()),
        });
        let composite_pipeline = device.create_compute_pipeline(&ComputePipelineDescriptor {
            label: Some("composite_pipeline"),
            layout: Some(&composite_pipeline_layout),
            module: &composite_shader,
            entry_point: Some("main"),
            compilation_options: Default::default(),
            cache: None,
        });

        Self {
            thresholding_layout,
            thresholding_pipeline,
            ccl_layout,
            ccl_merge_pipeline,
            ccl_compress_pipeline,
            ccl_extract_pipeline,
            grow_estimate_layout,
            grow_estimate_pipeline,
            grow_main_layout,
            grow_main_pipeline,
            smaa_layout,
            smaa_pipeline,
            composite_layout,
            composite_pipeline,
        }
    }

    pub fn dispatch(
        &self,
        device: &Device,
        queue: &Queue,
        params: &BucketParams,
        ref_layer: &LayerBindingData,
        ref_layer_tile_info: Vec<IVec2>,
        output_layer: &mut DynamicLayerStorage,
    ) {
        let dispatch_xy = GpuTileStorageInner::TILE_SIZE.div_ceil(16);
        let mut mask_tile_count = ref_layer.texture.texture().depth_or_array_layers();

        let mut params_buffer =
            DynamicBuffer::new(Some("bucket_params_buffer"), BufferUsages::UNIFORM);
        params_buffer.push(params);
        params_buffer.write_buffer(device, queue);

        let mut smaa_params_buffer =
            DynamicBuffer::new(Some("smaa_params_buffer"), BufferUsages::UNIFORM);
        smaa_params_buffer.push(&SmaaParams::default());
        smaa_params_buffer.write_buffer(device, queue);

        let total_pixels =
            mask_tile_count * GpuTileStorageInner::TILE_SIZE * GpuTileStorageInner::TILE_SIZE;

        let labels_buffer = device.create_buffer(&BufferDescriptor {
            label: Some("labels_buffer"),
            size: (total_pixels * 4) as u64,
            usage: BufferUsages::STORAGE,
            mapped_at_creation: false,
        });

        let mask_texture = device.create_texture(&TextureDescriptor {
            label: Some("mask_texture"),
            size: Extent3d {
                width: GpuTileStorageInner::TILE_SIZE,
                height: GpuTileStorageInner::TILE_SIZE,
                depth_or_array_layers: mask_tile_count,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: TextureDimension::D2,
            format: MASK_TEXTURE_FORMAT,
            usage: TextureUsages::STORAGE_BINDING,
            view_formats: &[],
        });
        let mut mask_texture_view = mask_texture.create_view(&TextureViewDescriptor {
            dimension: Some(TextureViewDimension::D2Array),
            ..Default::default()
        });
        let mut mask_tile_info_buffer = ref_layer.tile_info_buffer.clone();

        let mut ec = device.create_command_encoder(&Default::default());

        let thresholding_bind_group = device.create_bind_group(&BindGroupDescriptor {
            label: Some("thresholding_bind_group"),
            layout: &self.thresholding_layout,
            entries: &[
                BindGroupEntry {
                    binding: 0,
                    resource: params_buffer.binding().unwrap(),
                },
                BindGroupEntry {
                    binding: 1,
                    resource: BindingResource::TextureView(&ref_layer.texture),
                },
                BindGroupEntry {
                    binding: 2,
                    resource: labels_buffer.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 3,
                    resource: ref_layer.tile_info_buffer.as_entire_binding(),
                },
            ],
        });

        {
            let mut pass = ec.begin_compute_pass(&ComputePassDescriptor {
                label: Some("thresholding_pass"),
                timestamp_writes: None,
            });

            pass.set_pipeline(&self.thresholding_pipeline);
            pass.set_bind_group(0, &thresholding_bind_group, &[]);
            pass.dispatch_workgroups(dispatch_xy, dispatch_xy, mask_tile_count);
        }

        // unsafe { device.start_graphics_debugger_capture() };
        // debug_bit_mask(
        //     device,
        //     queue,
        //     &prepared.bit_mask,
        //     prepared.input_layer_tile_count,
        // );
        // unsafe { device.stop_graphics_debugger_capture() };

        let ccl_bind_group = device.create_bind_group(&BindGroupDescriptor {
            label: Some("ccl_bind_group"),
            layout: &self.ccl_layout,
            entries: &[
                BindGroupEntry {
                    binding: 0,
                    resource: params_buffer.binding().unwrap(),
                },
                BindGroupEntry {
                    binding: 1,
                    resource: BindingResource::TextureView(&mask_texture_view),
                },
                BindGroupEntry {
                    binding: 2,
                    resource: labels_buffer.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 3,
                    resource: mask_tile_info_buffer.as_entire_binding(),
                },
            ],
        });

        {
            let mut pass = ec.begin_compute_pass(&ComputePassDescriptor {
                label: Some("ccl_merge_pass"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&self.ccl_merge_pipeline);
            pass.set_bind_group(0, &ccl_bind_group, &[]);
            let max_distance = (total_pixels as f32).sqrt().ceil() as u32;
            let ccl_iterations = (max_distance as f32).log2().ceil() as u32 + 1;
            for _ in 0..ccl_iterations {
                pass.dispatch_workgroups(dispatch_xy, dispatch_xy, mask_tile_count);
            }
        }

        {
            let mut pass = ec.begin_compute_pass(&ComputePassDescriptor {
                label: Some("ccl_compress_pass"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&self.ccl_compress_pipeline);
            pass.set_bind_group(0, &ccl_bind_group, &[]);
            pass.dispatch_workgroups(dispatch_xy, dispatch_xy, mask_tile_count);
        }

        {
            let mut pass = ec.begin_compute_pass(&ComputePassDescriptor {
                label: Some("ccl_extract_pass"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&self.ccl_extract_pipeline);
            pass.set_bind_group(0, &ccl_bind_group, &[]);
            pass.dispatch_workgroups(dispatch_xy, dispatch_xy, mask_tile_count);
        }

        queue.submit([ec.finish()]);

        if params.grow > 0 {
            let mut ec = device.create_command_encoder(&Default::default());

            let estimate_buffer = device.create_buffer(&BufferDescriptor {
                label: Some("grow_estimate_buffer"),
                size: mask_tile_count as u64 * 4,
                usage: BufferUsages::STORAGE | BufferUsages::COPY_SRC,
                mapped_at_creation: false,
            });

            let estimate_readback_buffer = device.create_buffer(&BufferDescriptor {
                label: Some("grow_estimate_readback_buffer"),
                size: estimate_buffer.size(),
                usage: BufferUsages::MAP_READ | BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });

            let grow_estimate_bind_group = device.create_bind_group(&BindGroupDescriptor {
                label: Some("grow_estimate_bind_group"),
                layout: &self.grow_estimate_layout,
                entries: &[
                    BindGroupEntry {
                        binding: 0,
                        resource: params_buffer.binding().unwrap(),
                    },
                    BindGroupEntry {
                        binding: 1,
                        resource: BindingResource::TextureView(&mask_texture_view),
                    },
                    BindGroupEntry {
                        binding: 3,
                        resource: mask_tile_info_buffer.as_entire_binding(),
                    },
                    BindGroupEntry {
                        binding: 5,
                        resource: estimate_buffer.as_entire_binding(),
                    },
                ],
            });

            {
                let mut pass = ec.begin_compute_pass(&ComputePassDescriptor {
                    label: Some("grow_estimate_pass"),
                    timestamp_writes: None,
                });
                pass.set_pipeline(&self.grow_estimate_pipeline);
                pass.set_bind_group(0, &grow_estimate_bind_group, &[]);
                pass.dispatch_workgroups(dispatch_xy, dispatch_xy, mask_tile_count);
            }

            ec.copy_buffer_to_buffer(
                &estimate_buffer,
                0,
                &estimate_readback_buffer,
                0,
                estimate_buffer.size(),
            );

            let submission_index = queue.submit([ec.finish()]);
            let estimate_slice = estimate_readback_buffer.slice(..);
            let (tx, rx) = mpsc::channel();
            estimate_slice.map_async(MapMode::Read, move |result| {
                tx.send(result).ok();
            });
            device
                .poll(PollType::Wait {
                    submission_index: Some(submission_index),
                    timeout: None,
                })
                .unwrap();
            rx.recv().unwrap().unwrap();

            let mapped = estimate_slice.get_mapped_range();

            let mut candidate_allocate_tiles = IndexMap::new();
            let buffer = mapped.to_vec();
            let allocation_info = bytemuck::cast_slice::<_, u32>(&buffer);
            let mut new_tile_info = IndexMap::new();
            for (allocation_info, index) in allocation_info
                .iter()
                .zip(ref_layer_tile_info.iter().copied())
            {
                new_tile_info.insert(
                    index,
                    GpuTileInfo {
                        index: index,
                        origin: index * GpuTileStorageInner::TILE_SIZE as i32,
                    },
                );
                const OFFSETS: [IVec2; 8] = [
                    IVec2::new(-1, -1),
                    IVec2::new(0, -1),
                    IVec2::new(1, -1),
                    IVec2::new(1, 0),
                    IVec2::new(1, 1),
                    IVec2::new(0, 1),
                    IVec2::new(-1, 1),
                    IVec2::new(-1, 0),
                ];

                for (i, offset) in OFFSETS.iter().enumerate() {
                    if (*allocation_info >> i as u32) & 1 != 0 {
                        let index = index + *offset;
                        output_layer.get_tile_or_allocate(index);
                        candidate_allocate_tiles.insert(
                            index,
                            GpuTileInfo {
                                index,
                                origin: index * GpuTileStorageInner::TILE_SIZE as i32,
                            },
                        );
                    }
                }
            }
            drop(mapped);
            estimate_readback_buffer.unmap();

            new_tile_info.extend(candidate_allocate_tiles);
            let mut new_tile_info_buffer = BufferVec::new(
                Some("grown_tile_info_buffer".to_string()),
                BufferUsages::STORAGE,
            );
            for info in new_tile_info.values() {
                new_tile_info_buffer.push(info);
            }
            new_tile_info_buffer.write_buffer(device, queue);
            let new_tile_info_buffer = new_tile_info_buffer.into_inner_buffer().unwrap();

            let grown_mask_texture = device.create_texture(&TextureDescriptor {
                label: Some("grown_mask_texture"),
                size: Extent3d {
                    width: GpuTileStorageInner::TILE_SIZE,
                    height: GpuTileStorageInner::TILE_SIZE,
                    depth_or_array_layers: new_tile_info.len() as u32,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: TextureDimension::D2,
                format: MASK_TEXTURE_FORMAT,
                usage: TextureUsages::STORAGE_BINDING,
                view_formats: &[],
            });
            let grown_mask_texture_view = grown_mask_texture.create_view(&TextureViewDescriptor {
                dimension: Some(TextureViewDimension::D2Array),
                ..Default::default()
            });

            let grow_main_bind_group = device.create_bind_group(&BindGroupDescriptor {
                label: Some("grow_main_bind_group"),
                layout: &self.grow_main_layout,
                entries: &[
                    BindGroupEntry {
                        binding: 0,
                        resource: params_buffer.binding().unwrap(),
                    },
                    BindGroupEntry {
                        binding: 1,
                        resource: BindingResource::TextureView(&mask_texture_view),
                    },
                    BindGroupEntry {
                        binding: 2,
                        resource: BindingResource::TextureView(&grown_mask_texture_view),
                    },
                    BindGroupEntry {
                        binding: 3,
                        resource: ref_layer.tile_info_buffer.as_entire_binding(),
                    },
                    BindGroupEntry {
                        binding: 4,
                        resource: new_tile_info_buffer.as_entire_binding(),
                    },
                ],
            });

            let mut ec = device.create_command_encoder(&Default::default());

            {
                let mut pass = ec.begin_compute_pass(&ComputePassDescriptor {
                    label: Some("grow_main_pass"),
                    timestamp_writes: None,
                });
                pass.set_pipeline(&self.grow_main_pipeline);
                pass.set_bind_group(0, &grow_main_bind_group, &[]);
                pass.dispatch_workgroups(dispatch_xy, dispatch_xy, output_layer.len() as u32);
            }

            queue.submit([ec.finish()]);

            mask_texture_view = grown_mask_texture_view;
            mask_tile_info_buffer = new_tile_info_buffer;
            mask_tile_count = new_tile_info.len() as u32;
        }

        let mut ec = device.create_command_encoder(&Default::default());

        let smoothed_mask_texture = device.create_texture(&TextureDescriptor {
            label: Some("smoothed_mask_texture"),
            size: Extent3d {
                width: GpuTileStorageInner::TILE_SIZE,
                height: GpuTileStorageInner::TILE_SIZE,
                depth_or_array_layers: output_layer.len() as u32,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: TextureDimension::D2,
            format: MASK_TEXTURE_FORMAT,
            usage: TextureUsages::STORAGE_BINDING,
            view_formats: &[],
        });
        let smoothed_mask_texture_view = smoothed_mask_texture.create_view(&Default::default());

        let smaa_bind_group = device.create_bind_group(&BindGroupDescriptor {
            label: Some("smaa_bind_group"),
            layout: &self.smaa_layout,
            entries: &[
                BindGroupEntry {
                    binding: 0,
                    resource: BindingResource::TextureView(&mask_texture_view),
                },
                BindGroupEntry {
                    binding: 1,
                    resource: BindingResource::TextureView(&smoothed_mask_texture_view),
                },
                BindGroupEntry {
                    binding: 2,
                    resource: mask_tile_info_buffer.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 3,
                    resource: smaa_params_buffer.binding().unwrap(),
                },
            ],
        });

        {
            let mut pass = ec.begin_compute_pass(&ComputePassDescriptor {
                label: Some("smaa_pass"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&self.smaa_pipeline);
            pass.set_bind_group(0, &smaa_bind_group, &[]);
            pass.dispatch_workgroups(dispatch_xy, dispatch_xy, mask_tile_count);
        }

        for index in ref_layer_tile_info {
            output_layer.get_tile_or_allocate(index);
        }

        let (Some(output_texture), Some(output_tile_info)) =
            (output_layer.texture(), output_layer.tile_info_buffer())
        else {
            return;
        };

        let composite_bind_group = device.create_bind_group(&BindGroupDescriptor {
            label: Some("composite_bind_group"),
            layout: &self.composite_layout,
            entries: &[
                BindGroupEntry {
                    binding: 0,
                    resource: params_buffer.binding().unwrap(),
                },
                BindGroupEntry {
                    binding: 1,
                    resource: BindingResource::TextureView(&smoothed_mask_texture_view),
                },
                BindGroupEntry {
                    binding: 2,
                    resource: mask_tile_info_buffer.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 3,
                    resource: BindingResource::TextureView(&output_texture),
                },
                BindGroupEntry {
                    binding: 4,
                    resource: output_tile_info.as_entire_binding(),
                },
            ],
        });

        {
            let mut pass = ec.begin_compute_pass(&ComputePassDescriptor {
                label: Some("composite_pass"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&self.composite_pipeline);
            pass.set_bind_group(0, &composite_bind_group, &[]);
            pass.dispatch_workgroups(dispatch_xy, dispatch_xy, mask_tile_count);
        }

        unsafe { device.start_graphics_debugger_capture() };
        queue.submit([ec.finish()]);
        unsafe { device.stop_graphics_debugger_capture() };

        // unsafe { device.start_graphics_debugger_capture() };
        // debug_bit_mask(
        //     device,
        //     queue,
        //     &prepared.ccl_output,
        //     prepared.input_layer_tile_count,
        // );
        // unsafe { device.stop_graphics_debugger_capture() };
    }
}

fn debug_bit_mask(device: &Device, queue: &Queue, bit_mask: &Buffer, n_tiles: u32) {
    let debug_bit_mask_layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
        label: Some("debug_bit_mask_layout"),
        entries: &[
            BindGroupLayoutEntry {
                binding: 0,
                visibility: ShaderStages::COMPUTE,
                ty: BindingType::Buffer {
                    ty: BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: Some(f32::min_size()),
                },
                count: None,
            },
            BindGroupLayoutEntry {
                binding: 1,
                visibility: ShaderStages::COMPUTE,
                ty: BindingType::StorageTexture {
                    access: StorageTextureAccess::WriteOnly,
                    format: TextureFormat::Rgba8Unorm,
                    view_dimension: TextureViewDimension::D2Array,
                },
                count: None,
            },
        ],
    });

    let debug_bit_mask_pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
        label: Some("debug_bit_mask_pipeline_layout"),
        bind_group_layouts: &[&debug_bit_mask_layout],
        push_constant_ranges: &[],
    });
    let debug_bit_mask_shader = device.create_shader_module(ShaderModuleDescriptor {
        label: Some("debug_bit_mask_shader"),
        source: ShaderSource::Wgsl(include_wesl!("debug_bit_mask").into()),
    });
    let debug_bit_mask_pipeline = device.create_compute_pipeline(&ComputePipelineDescriptor {
        label: Some("debug_bit_mask_pipeline"),
        layout: Some(&debug_bit_mask_pipeline_layout),
        module: &debug_bit_mask_shader,
        entry_point: Some("main"),
        compilation_options: Default::default(),
        cache: None,
    });

    let output_tex = device.create_texture(&TextureDescriptor {
        label: Some("debug_bit_mask_output_tex"),
        size: Extent3d {
            width: GpuTileStorageInner::TILE_SIZE,
            height: GpuTileStorageInner::TILE_SIZE,
            depth_or_array_layers: n_tiles,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: TextureDimension::D2,
        format: TextureFormat::Rgba8Unorm,
        usage: TextureUsages::STORAGE_BINDING,
        view_formats: &[],
    });
    let bind_group = device.create_bind_group(&BindGroupDescriptor {
        label: Some("debug_bit_mask_bind_group"),
        layout: &debug_bit_mask_layout,
        entries: &[
            BindGroupEntry {
                binding: 0,
                resource: bit_mask.as_entire_binding(),
            },
            BindGroupEntry {
                binding: 1,
                resource: BindingResource::TextureView(
                    &output_tex.create_view(&Default::default()),
                ),
            },
        ],
    });

    let mut ec = device.create_command_encoder(&Default::default());
    {
        let mut pass = ec.begin_compute_pass(&ComputePassDescriptor {
            label: Some("debug_bit_mask_pass"),
            timestamp_writes: None,
        });
        pass.set_pipeline(&debug_bit_mask_pipeline);
        pass.set_bind_group(0, &bind_group, &[]);
        pass.dispatch_workgroups(
            GpuTileStorageInner::TILE_SIZE.div_ceil(16),
            GpuTileStorageInner::TILE_SIZE.div_ceil(16),
            n_tiles,
        );
    }
    queue.submit([ec.finish()]);
}
