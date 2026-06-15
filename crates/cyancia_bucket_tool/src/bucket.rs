use std::{
    collections::HashSet,
    sync::{Arc, mpsc},
};

use cyancia_image::{
    texel::TexelType,
    tile::{DynamicLayerStorage, GpuTileInfo, GpuTileStorageInner, LayerBindingData, TileIndex},
};
use cyancia_render::{
    buffer::{BufferVec, DynamicBuffer},
    readback::{
        create_readback_buffer_and_schedule_copy, readback_buffer_async,
        readback_buffer_on_submit_async,
    },
    util::DevicePollExt,
};
use encase::ShaderType;
use glam::{IVec2, UVec2, Vec4};
use indexmap::{IndexMap, IndexSet};
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
const ACTIVE_TILE_ALLOCATION_BIT: u32 = 1 << 8;

#[derive(Debug, Clone, Copy)]
pub struct BucketParams {
    pub seed: UVec2,
    pub fill_color: Vec4,
    pub threshold: f32,
    pub alpha_threshold: f32,
    pub close_gap: u32,
    pub grow: i32,
    pub image_size: UVec2,
}

#[derive(ShaderType, Debug, Clone, Copy)]
struct BucketParamsInner {
    pub seed: UVec2,
    pub fill_color: Vec4,
    pub threshold: f32,
    pub alpha_threshold: f32,
    pub close_gap: u32,
    pub grow: i32,
    pub image_size: UVec2,
    pub transparent_mode: u32,
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
    seed_mode_layout: BindGroupLayout,
    seed_mode_pipeline: ComputePipeline,
    thresholding_layout: BindGroupLayout,
    thresholding_pipeline: ComputePipeline,
    scan_pixels_layout: BindGroupLayout,
    scan_pixels_pipeline: ComputePipeline,
    close_gap_and_grow_layout: BindGroupLayout,
    close_gap_dilate_pipeline: ComputePipeline,
    close_gap_erode_pipeline: ComputePipeline,
    ccl_layout: BindGroupLayout,
    ccl_merge_pipeline: ComputePipeline,
    ccl_compress_pipeline: ComputePipeline,
    ccl_extract_pipeline: ComputePipeline,
    grow_pipeline: ComputePipeline,
    smaa_layout: BindGroupLayout,
    smaa_pipeline: ComputePipeline,
    composite_layout: BindGroupLayout,
    composite_pipeline: ComputePipeline,
}

impl Bucket {
    pub fn new(device: &Device, ref_texel_type: TexelType, output_texel_type: TexelType) -> Self {
        let seed_mode_layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some("seed_mode_layout"),
            entries: &[
                BindGroupLayoutEntry {
                    binding: 0,
                    visibility: ShaderStages::COMPUTE,
                    ty: BindingType::Buffer {
                        ty: BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: Some(BucketParamsInner::min_size()),
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

        let seed_mode_pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: Some("seed_mode_pipeline_layout"),
            bind_group_layouts: &[&seed_mode_layout],
            push_constant_ranges: &[],
        });
        let seed_mode_shader = device.create_shader_module(ShaderModuleDescriptor {
            label: Some("seed_mode_shader_module"),
            source: ShaderSource::Wgsl(include_wesl!("seed_mode").into()),
        });
        let seed_mode_pipeline = device.create_compute_pipeline(&ComputePipelineDescriptor {
            label: Some("seed_mode_pipeline"),
            layout: Some(&seed_mode_pipeline_layout),
            module: &seed_mode_shader,
            entry_point: Some("main"),
            compilation_options: Default::default(),
            cache: None,
        });

        let thresholding_layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some("thresholding_layout"),
            entries: &[
                BindGroupLayoutEntry {
                    binding: 0,
                    visibility: ShaderStages::COMPUTE,
                    ty: BindingType::Buffer {
                        ty: BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: Some(BucketParamsInner::min_size()),
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

        let scan_pixels_layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some("scan_pixels_layout"),
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
                    ty: BindingType::Buffer {
                        ty: BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: Some(GpuTileInfo::min_size()),
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
            ],
        });
        let scan_pixels_pipeline_layout =
            device.create_pipeline_layout(&PipelineLayoutDescriptor {
                label: Some("scan_pixels_pipeline_layout"),
                bind_group_layouts: &[&scan_pixels_layout],
                push_constant_ranges: &[],
            });
        let scan_pixels_shader = device.create_shader_module(ShaderModuleDescriptor {
            label: Some("scan_pixels_shader"),
            source: ShaderSource::Wgsl(include_wesl!("scan_pixels").into()),
        });
        let scan_pixels_pipeline = device.create_compute_pipeline(&ComputePipelineDescriptor {
            label: Some("scan_pixels_pipeline"),
            layout: Some(&scan_pixels_pipeline_layout),
            module: &scan_pixels_shader,
            entry_point: Some("main"),
            compilation_options: Default::default(),
            cache: None,
        });

        let close_gap_and_grow_shader = device.create_shader_module(ShaderModuleDescriptor {
            label: Some("close_gap_and_grow_shader"),
            source: ShaderSource::Wgsl(include_wesl!("close_gap_and_grow").into()),
        });

        let close_gap_and_grow_layout =
            device.create_bind_group_layout(&BindGroupLayoutDescriptor {
                label: Some("close_gap_and_grow_layout"),
                entries: &[
                    BindGroupLayoutEntry {
                        binding: 0,
                        visibility: ShaderStages::COMPUTE,
                        ty: BindingType::Buffer {
                            ty: BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: Some(BucketParamsInner::min_size()),
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
                ],
            });

        let close_gap_and_grow_pipeline_layout =
            device.create_pipeline_layout(&PipelineLayoutDescriptor {
                label: Some("close_gap_and_grow_pipeline_layout"),
                bind_group_layouts: &[&close_gap_and_grow_layout],
                push_constant_ranges: &[],
            });

        let close_gap_dilate_pipeline =
            device.create_compute_pipeline(&ComputePipelineDescriptor {
                label: Some("close_gap_dilate_pipeline"),
                layout: Some(&close_gap_and_grow_pipeline_layout),
                module: &close_gap_and_grow_shader,
                entry_point: Some("close_gap_dilate"),
                compilation_options: Default::default(),
                cache: None,
            });

        let close_gap_erode_pipeline = device.create_compute_pipeline(&ComputePipelineDescriptor {
            label: Some("close_gap_erode_pipeline"),
            layout: Some(&close_gap_and_grow_pipeline_layout),
            module: &close_gap_and_grow_shader,
            entry_point: Some("close_gap_erode"),
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
                        min_binding_size: Some(BucketParamsInner::min_size()),
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

        let grow_pipeline = device.create_compute_pipeline(&ComputePipelineDescriptor {
            label: Some("grow_pipeline"),
            layout: Some(&close_gap_and_grow_pipeline_layout),
            module: &close_gap_and_grow_shader,
            entry_point: Some("grow"),
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
                        min_binding_size: Some(BucketParamsInner::min_size()),
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
            seed_mode_layout,
            seed_mode_pipeline,
            thresholding_layout,
            thresholding_pipeline,
            scan_pixels_layout,
            scan_pixels_pipeline,
            close_gap_and_grow_layout,
            close_gap_dilate_pipeline,
            close_gap_erode_pipeline,
            ccl_layout,
            ccl_merge_pipeline,
            ccl_compress_pipeline,
            ccl_extract_pipeline,
            grow_pipeline,
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
        ref_layer_tile_info: IndexSet<IVec2>,
        output_layer: &mut DynamicLayerStorage,
    ) {
        let dispatch_xy = GpuTileStorageInner::TILE_SIZE.div_ceil(16);

        let mut params = BucketParamsInner {
            seed: params.seed,
            fill_color: params.fill_color,
            threshold: params.threshold,
            alpha_threshold: params.alpha_threshold,
            close_gap: params.close_gap,
            grow: params.grow,
            image_size: params.image_size,
            transparent_mode: 0,
        };

        let mut seed_params_buffer =
            DynamicBuffer::new(Some("bucket_seed_params_buffer"), BufferUsages::UNIFORM);
        seed_params_buffer.push(&params);
        seed_params_buffer.write_buffer(device, queue);

        let seed_transparent_mode =
            self.classify_seed_mode(device, queue, &seed_params_buffer, ref_layer);
        params.transparent_mode = u32::from(seed_transparent_mode);

        let mut params_buffer =
            DynamicBuffer::new(Some("bucket_params_buffer"), BufferUsages::UNIFORM);
        params_buffer.push(&params);
        params_buffer.write_buffer(device, queue);

        let (mut mask_tile_indices, mut mask_tile_info_buffer) = if seed_transparent_mode {
            let image_tile_count = GpuTileStorageInner::calc_tile_count(params.image_size);
            let mut mask_tile_indices =
                IndexSet::with_capacity(image_tile_count.element_product() as usize);
            let mut tile_info_buffer = BufferVec::new(
                Some("transparent_mask_tile_info_buffer".to_string()),
                BufferUsages::STORAGE,
            );

            for y in 0..image_tile_count.y {
                for x in 0..image_tile_count.x {
                    let index = IVec2::new(x as i32, y as i32);
                    mask_tile_indices.insert(index);
                    tile_info_buffer.push(&GpuTileInfo {
                        index,
                        origin: index * GpuTileStorageInner::TILE_SIZE as i32,
                    });
                }
            }

            tile_info_buffer.write_buffer(device, queue);
            (
                mask_tile_indices,
                tile_info_buffer.into_inner_buffer().unwrap(),
            )
        } else {
            (
                ref_layer_tile_info.clone(),
                ref_layer.tile_info_buffer.clone(),
            )
        };

        if mask_tile_indices.is_empty() {
            return;
        }

        let mut smaa_params_buffer =
            DynamicBuffer::new(Some("smaa_params_buffer"), BufferUsages::UNIFORM);
        smaa_params_buffer.push(&SmaaParams::default());
        smaa_params_buffer.write_buffer(device, queue);

        let total_pixels = mask_tile_indices.len() as u32
            * GpuTileStorageInner::TILE_SIZE
            * GpuTileStorageInner::TILE_SIZE;

        let labels_buffer = device.create_buffer(&BufferDescriptor {
            label: Some("labels_buffer"),
            size: (total_pixels * 4) as u64,
            usage: BufferUsages::STORAGE,
            mapped_at_creation: false,
        });

        let mut mask_texture = device.create_texture(&TextureDescriptor {
            label: Some("mask_texture"),
            size: Extent3d {
                width: GpuTileStorageInner::TILE_SIZE,
                height: GpuTileStorageInner::TILE_SIZE,
                depth_or_array_layers: mask_tile_indices.len() as u32,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: TextureDimension::D2,
            format: MASK_TEXTURE_FORMAT,
            usage: TextureUsages::STORAGE_BINDING | TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let mut mask_texture_view = mask_texture.create_view(&TextureViewDescriptor {
            dimension: Some(TextureViewDimension::D2Array),
            ..Default::default()
        });
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
                BindGroupEntry {
                    binding: 4,
                    resource: mask_tile_info_buffer.as_entire_binding(),
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
            pass.dispatch_workgroups(dispatch_xy, dispatch_xy, mask_tile_indices.len() as u32);
        }

        let is_not_empty_buffer = device.create_buffer(&BufferDescriptor {
            label: Some("is_not_empty_buffer"),
            size: mask_tile_indices.len() as u64 * 4,
            usage: BufferUsages::STORAGE | BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });

        let scan_pixels_bind_group = device.create_bind_group(&BindGroupDescriptor {
            label: Some("scan_pixels_bind_group"),
            layout: &self.scan_pixels_layout,
            entries: &[
                BindGroupEntry {
                    binding: 0,
                    resource: BindingResource::TextureView(&mask_texture_view),
                },
                BindGroupEntry {
                    binding: 1,
                    resource: mask_tile_info_buffer.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 2,
                    resource: is_not_empty_buffer.as_entire_binding(),
                },
            ],
        });

        {
            let mut pass = ec.begin_compute_pass(&ComputePassDescriptor::default());
            pass.set_pipeline(&self.scan_pixels_pipeline);
            pass.set_bind_group(0, &scan_pixels_bind_group, &[]);
            pass.dispatch_workgroups(dispatch_xy, dispatch_xy, mask_tile_indices.len() as u32);
        }

        let is_not_empty_readback =
            create_readback_buffer_and_schedule_copy(device, &mut ec, &is_not_empty_buffer);
        let is_not_empty_readback_async =
            readback_buffer_on_submit_async::<Vec<u32>, _>(&mut ec, &is_not_empty_readback, ..);

        let si = queue.submit([ec.finish()]);
        device.poll_indefinitely_for(si).unwrap();

        let is_not_empty = is_not_empty_readback_async.block_on().unwrap();
        for not_empty_index in is_not_empty {
            let not_empty_tile_index = *mask_tile_indices
                .get_index(not_empty_index as usize)
                .unwrap();
            for dx in -1..=1 {
                for dy in -1..=1 {
                    if dx == 0 && dy == 0 {
                        continue;
                    }

                    mask_tile_indices.insert(not_empty_tile_index + IVec2::new(dx, dy));
                }
            }
        }

        mask_tile_info_buffer = {
            let mut b = BufferVec::new(
                Some("mask_tile_info_buffer".to_string()),
                BufferUsages::STORAGE,
            );
            for index in mask_tile_indices.iter().copied() {
                b.push(&GpuTileInfo {
                    index: index,
                    origin: GpuTileStorageInner::TILE_SIZE as i32 * index,
                });
            }
            b.write_buffer(device, queue);
            b.into_inner_buffer().unwrap()
        };

        let mut ec = device.create_command_encoder(&Default::default());

        mask_texture = {
            let t = device.create_texture(&TextureDescriptor {
                label: Some("mask_texture"),
                size: Extent3d {
                    width: GpuTileStorageInner::TILE_SIZE,
                    height: GpuTileStorageInner::TILE_SIZE,
                    depth_or_array_layers: mask_tile_indices.len() as u32,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: TextureDimension::D2,
                format: MASK_TEXTURE_FORMAT,
                usage: TextureUsages::STORAGE_BINDING | TextureUsages::COPY_DST,
                view_formats: &[],
            });
            ec.copy_texture_to_texture(
                mask_texture.as_image_copy(),
                t.as_image_copy(),
                mask_texture.size(),
            );
            t
        };
        mask_texture_view = mask_texture.create_view(&TextureViewDescriptor {
            dimension: Some(TextureViewDimension::D2Array),
            ..Default::default()
        });

        for index in mask_tile_indices.iter().copied() {
            output_layer.get_tile_or_allocate(index);
        }

        // unsafe { device.start_graphics_debugger_capture() };
        // debug_bit_mask(
        //     device,
        //     queue,
        //     &prepared.bit_mask,
        //     prepared.input_layer_tile_count,
        // );
        // unsafe { device.stop_graphics_debugger_capture() };

        let close_gap_intermediate_texture = device.create_texture(&TextureDescriptor {
            label: Some("close_gap_intermediate_texture"),
            size: mask_texture.size(),
            mip_level_count: 1,
            sample_count: 1,
            dimension: TextureDimension::D2,
            format: MASK_TEXTURE_FORMAT,
            usage: TextureUsages::STORAGE_BINDING,
            view_formats: &[],
        });
        let close_gap_intermediate_texture_view =
            close_gap_intermediate_texture.create_view(&TextureViewDescriptor {
                dimension: Some(TextureViewDimension::D2Array),
                ..Default::default()
            });

        let close_gap_dilate_bind_group = device.create_bind_group(&BindGroupDescriptor {
            label: Some("close_gap_dilate_bind_group"),
            layout: &self.close_gap_and_grow_layout,
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
                    resource: BindingResource::TextureView(&close_gap_intermediate_texture_view),
                },
                BindGroupEntry {
                    binding: 3,
                    resource: mask_tile_info_buffer.as_entire_binding(),
                },
            ],
        });
        let close_gap_erode_bind_group = device.create_bind_group(&BindGroupDescriptor {
            label: Some("close_gap_erode_bind_group"),
            layout: &self.close_gap_and_grow_layout,
            entries: &[
                BindGroupEntry {
                    binding: 0,
                    resource: params_buffer.binding().unwrap(),
                },
                BindGroupEntry {
                    binding: 1,
                    resource: BindingResource::TextureView(&close_gap_intermediate_texture_view),
                },
                BindGroupEntry {
                    binding: 2,
                    resource: BindingResource::TextureView(&mask_texture_view),
                },
                BindGroupEntry {
                    binding: 3,
                    resource: mask_tile_info_buffer.as_entire_binding(),
                },
            ],
        });

        {
            let mut pass = ec.begin_compute_pass(&ComputePassDescriptor::default());
            pass.set_pipeline(&self.close_gap_erode_pipeline);

            pass.set_bind_group(0, &close_gap_dilate_bind_group, &[]);
            pass.dispatch_workgroups(dispatch_xy, dispatch_xy, mask_tile_indices.len() as u32);
            pass.set_bind_group(0, &close_gap_erode_bind_group, &[]);
            pass.dispatch_workgroups(dispatch_xy, dispatch_xy, mask_tile_indices.len() as u32);
        }

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
                pass.dispatch_workgroups(dispatch_xy, dispatch_xy, mask_tile_indices.len() as u32);
            }
        }

        {
            let mut pass = ec.begin_compute_pass(&ComputePassDescriptor {
                label: Some("ccl_compress_pass"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&self.ccl_compress_pipeline);
            pass.set_bind_group(0, &ccl_bind_group, &[]);
            pass.dispatch_workgroups(dispatch_xy, dispatch_xy, mask_tile_indices.len() as u32);
        }

        {
            let mut pass = ec.begin_compute_pass(&ComputePassDescriptor {
                label: Some("ccl_extract_pass"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&self.ccl_extract_pipeline);
            pass.set_bind_group(0, &ccl_bind_group, &[]);
            pass.dispatch_workgroups(dispatch_xy, dispatch_xy, mask_tile_indices.len() as u32);
        }

        queue.submit([ec.finish()]);

        if params.grow > 0 {
            let grown_mask_texture = device.create_texture(&TextureDescriptor {
                label: Some("grown_mask_texture"),
                size: Extent3d {
                    width: GpuTileStorageInner::TILE_SIZE,
                    height: GpuTileStorageInner::TILE_SIZE,
                    depth_or_array_layers: mask_tile_indices.len() as u32,
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

            let grow_bind_group = device.create_bind_group(&BindGroupDescriptor {
                label: Some("grow_bind_group"),
                layout: &self.close_gap_and_grow_layout,
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
                        resource: mask_tile_info_buffer.as_entire_binding(),
                    },
                ],
            });

            let mut ec = device.create_command_encoder(&Default::default());

            {
                let mut pass = ec.begin_compute_pass(&ComputePassDescriptor {
                    label: Some("grow_pass"),
                    timestamp_writes: None,
                });
                pass.set_pipeline(&self.grow_pipeline);
                pass.set_bind_group(0, &grow_bind_group, &[]);
                pass.dispatch_workgroups(dispatch_xy, dispatch_xy, mask_tile_indices.len() as u32);
            }

            queue.submit([ec.finish()]);

            mask_texture = grown_mask_texture;
            mask_texture_view = grown_mask_texture_view;
        }

        let mut ec = device.create_command_encoder(&Default::default());

        let smoothed_mask_texture = device.create_texture(&TextureDescriptor {
            label: Some("smoothed_mask_texture"),
            size: Extent3d {
                width: GpuTileStorageInner::TILE_SIZE,
                height: GpuTileStorageInner::TILE_SIZE,
                depth_or_array_layers: mask_tile_indices.len() as u32,
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
            pass.dispatch_workgroups(dispatch_xy, dispatch_xy, mask_tile_indices.len() as u32);
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
            pass.dispatch_workgroups(dispatch_xy, dispatch_xy, mask_tile_indices.len() as u32);
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

    fn classify_seed_mode(
        &self,
        device: &Device,
        queue: &Queue,
        params_buffer: &DynamicBuffer<BucketParamsInner>,
        ref_layer: &LayerBindingData,
    ) -> bool {
        let seed_mode_buffer = device.create_buffer(&BufferDescriptor {
            label: Some("seed_mode_buffer"),
            size: 4,
            usage: BufferUsages::STORAGE | BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });

        let seed_mode_bind_group = device.create_bind_group(&BindGroupDescriptor {
            label: Some("seed_mode_bind_group"),
            layout: &self.seed_mode_layout,
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
                    resource: seed_mode_buffer.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 3,
                    resource: ref_layer.tile_info_buffer.as_entire_binding(),
                },
            ],
        });

        let mut ec = device.create_command_encoder(&Default::default());
        {
            let mut pass = ec.begin_compute_pass(&ComputePassDescriptor {
                label: Some("seed_mode_pass"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&self.seed_mode_pipeline);
            pass.set_bind_group(0, &seed_mode_bind_group, &[]);
            pass.dispatch_workgroups(1, 1, 1);
        }

        let seed_mode_readback_buffer =
            create_readback_buffer_and_schedule_copy(device, &mut ec, &seed_mode_buffer);
        let seed_mode_readback =
            readback_buffer_on_submit_async::<u32, _>(&mut ec, &seed_mode_readback_buffer, ..);

        let si = queue.submit([ec.finish()]);
        device.poll_indefinitely_for(si).unwrap();

        let seed_mode = seed_mode_readback.block_on().unwrap();

        seed_mode == 1
    }
}

fn tile_info_for_index(index: IVec2) -> GpuTileInfo {
    GpuTileInfo {
        index,
        origin: index * GpuTileStorageInner::TILE_SIZE as i32,
    }
}

fn tile_intersects_image(index: IVec2, image_size: UVec2) -> bool {
    if index.x < 0 || index.y < 0 {
        return false;
    }

    let tile_min = UVec2::new(index.x as u32, index.y as u32) * GpuTileStorageInner::TILE_SIZE;
    tile_min.x < image_size.x && tile_min.y < image_size.y
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
