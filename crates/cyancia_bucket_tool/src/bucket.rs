use cyancia_image::{
    texel::TexelType,
    tile::{DynamicLayerStorage, GpuTileInfo, GpuTileStorageInner, LayerBindingData},
};
use cyancia_render::{
    buffer::{BufferVec, DynamicBuffer},
    readback::{create_readback_buffer_and_schedule_copy, readback_buffer_on_submit_async},
    util::DevicePollExt,
};
use encase::ShaderType;
use glam::{IVec2, UVec2, Vec4};
use indexmap::IndexSet;
use tracing::info;
use wesl::include_wesl;
use wgpu::{
    BindGroupDescriptor, BindGroupEntry, BindGroupLayout, BindGroupLayoutDescriptor,
    BindGroupLayoutEntry, BindingResource, BindingType, Buffer, BufferBindingType, BufferUsages,
    ComputePassDescriptor, ComputePipeline, ComputePipelineDescriptor, Device, Extent3d,
    PipelineLayoutDescriptor, Queue, ShaderModuleDescriptor, ShaderSource, ShaderStages,
    StorageTextureAccess, TextureDimension, TextureFormat, TextureUsages, TextureViewDimension,
    wgt::{BufferDescriptor, TextureDescriptor, TextureViewDescriptor},
};

// TODO Using packed buffer indeed reduced vram usage, but makes it almost impossible for renderdoc debug.

#[derive(Debug, Clone, Copy)]
pub enum BucketAntialiasApproach {
    None,
    Fxaa,
    Feather(u32),
}

#[derive(Debug, Clone, Copy)]
pub struct BucketParams {
    pub seed: UVec2,
    pub fill_color: Vec4,
    pub threshold: f32,
    pub alpha_threshold: f32,
    pub close_gap: u32,
    pub grow: i32,
    pub aa_approach: BucketAntialiasApproach,
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
    pub feather: u32,
    pub image_size: UVec2,
    pub transparent_mode: u32,
}

pub struct FxaaParams {
    pub edge_threshold_min: f32,
    pub edge_threshold_max: f32,
    pub iterations: u32,
    pub subpixel_quality: f32,
}

impl Default for FxaaParams {
    fn default() -> Self {
        Self::HIGH
    }
}

impl FxaaParams {
    pub const LOW: Self = Self {
        edge_threshold_min: 0.0833,
        edge_threshold_max: 0.250,
        iterations: 12,
        subpixel_quality: 0.75,
    };

    pub const MEDIUM: Self = Self {
        edge_threshold_min: 0.0625,
        edge_threshold_max: 0.166,
        iterations: 12,
        subpixel_quality: 0.75,
    };

    pub const HIGH: Self = Self {
        edge_threshold_min: 0.0312,
        edge_threshold_max: 0.125,
        iterations: 12,
        subpixel_quality: 0.75,
    };

    pub const ULTRA: Self = Self {
        edge_threshold_min: 0.0156,
        edge_threshold_max: 0.063,
        iterations: 12,
        subpixel_quality: 0.75,
    };

    pub const EXTREME: Self = Self {
        edge_threshold_min: 0.0078,
        edge_threshold_max: 0.031,
        iterations: 12,
        subpixel_quality: 0.75,
    };
}

#[derive(ShaderType, Debug, Clone, Copy)]
struct FxaaParamsInner {
    edge_threshold_min: f32,
    edge_threshold_max: f32,
    iterations: u32,
    subpixel_quality: f32,
    image_size: UVec2,
}

#[derive(ShaderType, Debug, Clone, Copy)]
pub struct JumpParams {
    pub jump: u32,
}

pub struct Bucket {
    seed_mode_layout: BindGroupLayout,
    seed_mode_pipeline: ComputePipeline,
    thresholding_layout: BindGroupLayout,
    thresholding_pipeline: ComputePipeline,
    scan_pixels_layout: BindGroupLayout,
    scan_pixels_pipeline: ComputePipeline,
    close_gap_resolve_pipeline: ComputePipeline,
    ccl_layout: BindGroupLayout,
    ccl_init_pipeline: ComputePipeline,
    ccl_merge_pipeline: ComputePipeline,
    ccl_compress_pipeline: ComputePipeline,
    ccl_extract_pipeline: ComputePipeline,
    grow_layout: BindGroupLayout,
    grow_pipeline: ComputePipeline,
    fxaa_layout: BindGroupLayout,
    fxaa_pipeline: ComputePipeline,
    close_gap_and_feather_layout: BindGroupLayout,
    close_gap_and_feather_seed_pipeline: ComputePipeline,
    close_gap_and_feather_jump_pipeline: ComputePipeline,
    feather_resolve_pipeline: ComputePipeline,
    composite_layout: BindGroupLayout,
    composite_pipeline: ComputePipeline,
}

impl std::fmt::Debug for Bucket {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Bucket").finish()
    }
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
            bind_group_layouts: &[Some(&seed_mode_layout)],
            ..Default::default()
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
                bind_group_layouts: &[Some(&thresholding_layout)],
                ..Default::default()
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
                    ty: BindingType::Buffer {
                        ty: BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: Some(u32::min_size()),
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
                bind_group_layouts: &[Some(&scan_pixels_layout)],
                ..Default::default()
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
                    ty: BindingType::Buffer {
                        ty: BufferBindingType::Storage { read_only: false },
                        has_dynamic_offset: false,
                        min_binding_size: Some(u32::min_size()),
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
            bind_group_layouts: &[Some(&ccl_layout)],
            ..Default::default()
        });

        let ccl_shader = device.create_shader_module(ShaderModuleDescriptor {
            label: Some("ccl_shader"),
            source: ShaderSource::Wgsl(include_wesl!("ccl").into()),
        });

        let ccl_init_pipeline = device.create_compute_pipeline(&ComputePipelineDescriptor {
            label: Some("ccl_init_pipeline"),
            layout: Some(&ccl_pipeline_layout),
            module: &ccl_shader,
            entry_point: Some("ccl_init"),
            compilation_options: Default::default(),
            cache: None,
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

        let grow_shader = device.create_shader_module(ShaderModuleDescriptor {
            label: Some("grow_shader"),
            source: ShaderSource::Wgsl(include_wesl!("grow").into()),
        });

        let grow_layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some("grow_layout"),
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
                    ty: BindingType::Buffer {
                        ty: BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: Some(u32::min_size()),
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

        let grow_pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: Some("grow_pipeline_layout"),
            bind_group_layouts: &[Some(&grow_layout)],
            ..Default::default()
        });

        let grow_pipeline = device.create_compute_pipeline(&ComputePipelineDescriptor {
            label: Some("grow_pipeline"),
            layout: Some(&grow_pipeline_layout),
            module: &grow_shader,
            entry_point: Some("grow"),
            compilation_options: Default::default(),
            cache: None,
        });

        let fxaa_layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some("fxaa_layout"),
            entries: &[
                BindGroupLayoutEntry {
                    binding: 0,
                    visibility: ShaderStages::COMPUTE,
                    ty: BindingType::Buffer {
                        ty: BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: Some(u32::min_size()),
                    },
                    count: None,
                },
                BindGroupLayoutEntry {
                    binding: 1,
                    visibility: ShaderStages::COMPUTE,
                    ty: BindingType::Buffer {
                        ty: BufferBindingType::Storage { read_only: false },
                        has_dynamic_offset: false,
                        min_binding_size: Some(u32::min_size()),
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
                        min_binding_size: Some(FxaaParamsInner::min_size()),
                    },
                    count: None,
                },
            ],
        });

        let fxaa_pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: Some("fxaa_pipeline_layout"),
            bind_group_layouts: &[Some(&fxaa_layout)],
            ..Default::default()
        });
        let fxaa_shader = device.create_shader_module(ShaderModuleDescriptor {
            label: Some("fxaa_shader"),
            source: ShaderSource::Wgsl(include_wesl!("fxaa").into()),
        });
        let fxaa_pipeline = device.create_compute_pipeline(&ComputePipelineDescriptor {
            label: Some("fxaa_pipeline"),
            layout: Some(&fxaa_pipeline_layout),
            module: &fxaa_shader,
            entry_point: Some("main"),
            compilation_options: Default::default(),
            cache: None,
        });

        let close_gap_and_feather_layout =
            device.create_bind_group_layout(&BindGroupLayoutDescriptor {
                label: Some("close_gap_and_feather_layout"),
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
                        ty: BindingType::Buffer {
                            ty: BufferBindingType::Storage { read_only: false },
                            has_dynamic_offset: false,
                            min_binding_size: Some(u32::min_size()),
                        },
                        count: None,
                    },
                    BindGroupLayoutEntry {
                        binding: 2,
                        visibility: ShaderStages::COMPUTE,
                        ty: BindingType::StorageTexture {
                            access: StorageTextureAccess::ReadOnly,
                            format: TextureFormat::Rg32Float,
                            view_dimension: TextureViewDimension::D2Array,
                        },
                        count: None,
                    },
                    BindGroupLayoutEntry {
                        binding: 3,
                        visibility: ShaderStages::COMPUTE,
                        ty: BindingType::StorageTexture {
                            access: StorageTextureAccess::WriteOnly,
                            format: TextureFormat::Rg32Float,
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
                    BindGroupLayoutEntry {
                        binding: 5,
                        visibility: ShaderStages::COMPUTE,
                        ty: BindingType::Buffer {
                            ty: BufferBindingType::Uniform,
                            has_dynamic_offset: true,
                            min_binding_size: Some(JumpParams::min_size()),
                        },
                        count: None,
                    },
                ],
            });

        let close_gap_and_feather_pipeline_layout =
            device.create_pipeline_layout(&PipelineLayoutDescriptor {
                label: Some("close_gap_and_feather_pipeline_layout"),
                bind_group_layouts: &[Some(&close_gap_and_feather_layout)],
                ..Default::default()
            });
        let close_gap_and_feather_shader = device.create_shader_module(ShaderModuleDescriptor {
            label: Some("close_gap_and_feather_shader"),
            source: ShaderSource::Wgsl(include_wesl!("close_gap_and_feather").into()),
        });
        let close_gap_and_feather_seed_pipeline =
            device.create_compute_pipeline(&ComputePipelineDescriptor {
                label: Some("close_gap_and_feather_seed_pipeline"),
                layout: Some(&close_gap_and_feather_pipeline_layout),
                module: &close_gap_and_feather_shader,
                entry_point: Some("seed_edges"),
                compilation_options: Default::default(),
                cache: None,
            });
        let close_gap_and_feather_jump_pipeline =
            device.create_compute_pipeline(&ComputePipelineDescriptor {
                label: Some("close_gap_and_feather_jump_pipeline"),
                layout: Some(&close_gap_and_feather_pipeline_layout),
                module: &close_gap_and_feather_shader,
                entry_point: Some("jfa_jump"),
                compilation_options: Default::default(),
                cache: None,
            });
        let feather_resolve_pipeline = device.create_compute_pipeline(&ComputePipelineDescriptor {
            label: Some("feather_resolve_pipeline"),
            layout: Some(&close_gap_and_feather_pipeline_layout),
            module: &close_gap_and_feather_shader,
            entry_point: Some("feather_resolve_alpha"),
            compilation_options: Default::default(),
            cache: None,
        });
        let close_gap_resolve_pipeline =
            device.create_compute_pipeline(&ComputePipelineDescriptor {
                label: Some("close_gap_resolve_pipeline"),
                layout: Some(&close_gap_and_feather_pipeline_layout),
                module: &close_gap_and_feather_shader,
                entry_point: Some("close_gap_resolve"),
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
                    ty: BindingType::Buffer {
                        ty: BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: Some(u32::min_size()),
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
            bind_group_layouts: &[Some(&composite_layout)],
            ..Default::default()
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
            close_gap_resolve_pipeline,
            ccl_layout,
            ccl_init_pipeline,
            ccl_merge_pipeline,
            ccl_compress_pipeline,
            ccl_extract_pipeline,
            grow_layout,
            grow_pipeline,
            fxaa_layout,
            fxaa_pipeline,
            close_gap_and_feather_layout,
            close_gap_and_feather_seed_pipeline,
            close_gap_and_feather_jump_pipeline,
            feather_resolve_pipeline,
            composite_layout,
            composite_pipeline,
        }
    }

    #[tracing::instrument(name = "bucket_dispatch", skip_all)]
    pub fn dispatch(
        &self,
        device: &Device,
        queue: &Queue,
        bucket_params: &BucketParams,
        ref_layer: &LayerBindingData,
        ref_layer_tile_info: IndexSet<IVec2>,
        output_layer: &mut DynamicLayerStorage,
    ) -> Vec<IVec2> {
        unsafe { device.start_graphics_debugger_capture() };
        let dispatch_xy = GpuTileStorageInner::TILE_SIZE.div_ceil(16);

        let mut inner_params = BucketParamsInner {
            seed: bucket_params.seed,
            fill_color: bucket_params.fill_color,
            threshold: bucket_params.threshold,
            alpha_threshold: bucket_params.alpha_threshold,
            close_gap: bucket_params.close_gap,
            grow: bucket_params.grow,
            feather: match bucket_params.aa_approach {
                BucketAntialiasApproach::Feather(f) => f,
                _ => 0,
            },
            image_size: bucket_params.image_size,
            transparent_mode: 0,
        };

        let mut seed_params_buffer =
            DynamicBuffer::new(Some("bucket_seed_params_buffer"), BufferUsages::UNIFORM);
        seed_params_buffer.push(&inner_params);
        seed_params_buffer.write_buffer(device, queue);

        let seed_transparent_mode =
            self.classify_seed_mode(device, queue, &seed_params_buffer, ref_layer);
        inner_params.transparent_mode = u32::from(seed_transparent_mode);

        let mut bucket_params_buffer =
            DynamicBuffer::new(Some("bucket_params_buffer"), BufferUsages::UNIFORM);
        bucket_params_buffer.push(&inner_params);
        bucket_params_buffer.write_buffer(device, queue);

        let (mut mask_tile_indices, mut mask_tile_info_buffer) = if seed_transparent_mode {
            let image_tile_count = GpuTileStorageInner::calc_tile_count(inner_params.image_size);
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
            return Vec::new();
        }

        let mut mask_buffer = device.create_buffer(&BufferDescriptor {
            label: Some("mask_buffer"),
            size: mask_buffer_size(mask_tile_indices.len() as u32),
            usage: BufferUsages::STORAGE | BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let mut ec = device.create_command_encoder(&Default::default());

        let thresholding_bind_group = device.create_bind_group(&BindGroupDescriptor {
            label: Some("thresholding_bind_group"),
            layout: &self.thresholding_layout,
            entries: &[
                BindGroupEntry {
                    binding: 0,
                    resource: bucket_params_buffer.binding().unwrap(),
                },
                BindGroupEntry {
                    binding: 1,
                    resource: BindingResource::TextureView(&ref_layer.texture),
                },
                BindGroupEntry {
                    binding: 2,
                    resource: mask_buffer.as_entire_binding(),
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

        queue.submit([ec.finish()]);

        let existing_mask_tiles = self.get_unempty_tiles(
            device,
            queue,
            &mask_buffer,
            &mask_tile_info_buffer,
            &mask_tile_indices,
        );
        for index in existing_mask_tiles {
            for dx in -1..=1 {
                for dy in -1..=1 {
                    if dx == 0 && dy == 0 {
                        continue;
                    }

                    mask_tile_indices.insert(index + IVec2::new(dx, dy));
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
                    index,
                    origin: GpuTileStorageInner::TILE_SIZE as i32 * index,
                });
            }
            b.write_buffer(device, queue);
            b.into_inner_buffer().unwrap()
        };

        let mut ec = device.create_command_encoder(&Default::default());

        mask_buffer = {
            let b = device.create_buffer(&BufferDescriptor {
                label: Some("mask_buffer"),
                size: mask_buffer_size(mask_tile_indices.len() as u32),
                usage: BufferUsages::STORAGE | BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            ec.copy_buffer_to_buffer(&mask_buffer, 0, &b, 0, mask_buffer.size());
            b
        };

        queue.submit([ec.finish()]);

        let seed_texture_a_view = {
            let t = device.create_texture(&TextureDescriptor {
                label: Some("feather_seed_texture_a"),
                size: Extent3d {
                    width: GpuTileStorageInner::TILE_SIZE,
                    height: GpuTileStorageInner::TILE_SIZE,
                    depth_or_array_layers: mask_tile_indices.len() as u32,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: TextureDimension::D2,
                format: TextureFormat::Rg32Float,
                usage: TextureUsages::STORAGE_BINDING,
                view_formats: &[],
            });
            t.create_view(&TextureViewDescriptor {
                dimension: Some(TextureViewDimension::D2Array),
                ..Default::default()
            })
        };
        let seed_texture_b_view = {
            let seed_texture_b = device.create_texture(&TextureDescriptor {
                label: Some("feather_seed_texture_b"),
                size: Extent3d {
                    width: GpuTileStorageInner::TILE_SIZE,
                    height: GpuTileStorageInner::TILE_SIZE,
                    depth_or_array_layers: mask_tile_indices.len() as u32,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: TextureDimension::D2,
                format: TextureFormat::Rg32Float,
                usage: TextureUsages::STORAGE_BINDING,
                view_formats: &[],
            });
            seed_texture_b.create_view(&TextureViewDescriptor {
                dimension: Some(TextureViewDimension::D2Array),
                ..Default::default()
            })
        };

        if inner_params.close_gap > 0 {
            let max_jump = inner_params.close_gap.next_power_of_two();
            let (jump_params_buffer, jump_params_offsets) =
                create_jfa_params(device, queue, max_jump, "close_gap_jump_params");
            let jump_iterations = jump_params_offsets.len();

            let common_entries = vec![
                BindGroupEntry {
                    binding: 0,
                    resource: bucket_params_buffer.binding().unwrap(),
                },
                BindGroupEntry {
                    binding: 1,
                    resource: mask_buffer.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 4,
                    resource: mask_tile_info_buffer.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 5,
                    resource: jump_params_buffer.binding().unwrap(),
                },
            ];

            let bind_groups = {
                let a_to_b_entries = common_entries
                    .clone()
                    .into_iter()
                    .chain([
                        BindGroupEntry {
                            binding: 2,
                            resource: BindingResource::TextureView(&seed_texture_a_view),
                        },
                        BindGroupEntry {
                            binding: 3,
                            resource: BindingResource::TextureView(&seed_texture_b_view),
                        },
                    ])
                    .collect::<Vec<_>>();
                let b_to_a_entries = common_entries
                    .into_iter()
                    .chain([
                        BindGroupEntry {
                            binding: 2,
                            resource: BindingResource::TextureView(&seed_texture_b_view),
                        },
                        BindGroupEntry {
                            binding: 3,
                            resource: BindingResource::TextureView(&seed_texture_a_view),
                        },
                    ])
                    .collect::<Vec<_>>();

                let a_to_b_group = device.create_bind_group(&BindGroupDescriptor {
                    label: Some("close_gap_seed_a_to_b_bind_group"),
                    layout: &self.close_gap_and_feather_layout,
                    entries: &a_to_b_entries,
                });
                let b_to_a_group = device.create_bind_group(&BindGroupDescriptor {
                    label: Some("close_gap_seed_b_to_a_bind_group"),
                    layout: &self.close_gap_and_feather_layout,
                    entries: &b_to_a_entries,
                });

                [a_to_b_group, b_to_a_group]
            };

            let mut ec = device.create_command_encoder(&Default::default());
            {
                let mut pass = ec.begin_compute_pass(&ComputePassDescriptor {
                    label: Some("close_gap_seed_edges_pass"),
                    timestamp_writes: None,
                });
                pass.set_pipeline(&self.close_gap_and_feather_seed_pipeline);
                pass.set_bind_group(0, &bind_groups[1], &[jump_params_offsets[0]]);
                pass.dispatch_workgroups(dispatch_xy, dispatch_xy, mask_tile_indices.len() as u32);
            }

            {
                let mut pass = ec.begin_compute_pass(&ComputePassDescriptor {
                    label: Some("close_gap_jfa_pass"),
                    timestamp_writes: None,
                });
                pass.set_pipeline(&self.close_gap_and_feather_jump_pipeline);
                for i in 0..jump_iterations {
                    pass.set_bind_group(0, &bind_groups[i % 2], &[jump_params_offsets[i]]);
                    pass.dispatch_workgroups(
                        dispatch_xy,
                        dispatch_xy,
                        mask_tile_indices.len() as u32,
                    );
                }
            }

            {
                let mut pass = ec.begin_compute_pass(&ComputePassDescriptor {
                    label: Some("close_gap_resolve_pass"),
                    timestamp_writes: None,
                });
                pass.set_pipeline(&self.close_gap_resolve_pipeline);
                pass.set_bind_group(
                    0,
                    &bind_groups[jump_iterations % 2],
                    &[jump_params_offsets[jump_iterations - 1]],
                );
                pass.dispatch_workgroups(dispatch_xy, dispatch_xy, mask_tile_indices.len() as u32);
            }

            queue.submit([ec.finish()]);
        }

        let total_pixels = mask_tile_indices.len() as u32
            * GpuTileStorageInner::TILE_SIZE
            * GpuTileStorageInner::TILE_SIZE;

        let labels_buffer = device.create_buffer(&BufferDescriptor {
            label: Some("labels_buffer"),
            size: (total_pixels * 4) as u64,
            usage: BufferUsages::STORAGE,
            mapped_at_creation: false,
        });

        let ccl_bind_group = device.create_bind_group(&BindGroupDescriptor {
            label: Some("ccl_bind_group"),
            layout: &self.ccl_layout,
            entries: &[
                BindGroupEntry {
                    binding: 0,
                    resource: bucket_params_buffer.binding().unwrap(),
                },
                BindGroupEntry {
                    binding: 1,
                    resource: mask_buffer.as_entire_binding(),
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

        let mut ec = device.create_command_encoder(&Default::default());

        {
            let mut pass = ec.begin_compute_pass(&ComputePassDescriptor {
                label: Some("ccl_pass"),
                timestamp_writes: None,
            });

            pass.push_debug_group("ccl_init_pass");
            pass.set_pipeline(&self.ccl_init_pipeline);
            pass.set_bind_group(0, &ccl_bind_group, &[]);
            pass.dispatch_workgroups(dispatch_xy, dispatch_xy, mask_tile_indices.len() as u32);
            pass.pop_debug_group();

            pass.push_debug_group("ccl_merge_pass");
            pass.set_pipeline(&self.ccl_merge_pipeline);
            pass.set_bind_group(0, &ccl_bind_group, &[]);
            let max_distance = (total_pixels as f32).sqrt().ceil() as u32;
            let ccl_iterations = (max_distance as f32).log2().ceil() as u32 + 1;
            for _ in 0..ccl_iterations {
                pass.dispatch_workgroups(dispatch_xy, dispatch_xy, mask_tile_indices.len() as u32);
            }
            pass.pop_debug_group();

            pass.push_debug_group("ccl_compress_pass");
            pass.set_pipeline(&self.ccl_compress_pipeline);
            pass.set_bind_group(0, &ccl_bind_group, &[]);
            pass.dispatch_workgroups(dispatch_xy, dispatch_xy, mask_tile_indices.len() as u32);
            pass.pop_debug_group();

            pass.push_debug_group("ccl_extract_pass");
            pass.set_pipeline(&self.ccl_extract_pipeline);
            pass.set_bind_group(0, &ccl_bind_group, &[]);
            pass.dispatch_workgroups(dispatch_xy, dispatch_xy, mask_tile_indices.len() as u32);
            pass.pop_debug_group();
        }

        queue.submit([ec.finish()]);

        if inner_params.grow > 0 {
            let grown_mask_buffer = device.create_buffer(&BufferDescriptor {
                label: Some("grown_mask_buffer"),
                size: mask_buffer_size(mask_tile_indices.len() as u32),
                usage: BufferUsages::STORAGE,
                mapped_at_creation: false,
            });

            let grow_bind_group = device.create_bind_group(&BindGroupDescriptor {
                label: Some("grow_bind_group"),
                layout: &self.grow_layout,
                entries: &[
                    BindGroupEntry {
                        binding: 0,
                        resource: bucket_params_buffer.binding().unwrap(),
                    },
                    BindGroupEntry {
                        binding: 1,
                        resource: mask_buffer.as_entire_binding(),
                    },
                    BindGroupEntry {
                        binding: 2,
                        resource: grown_mask_buffer.as_entire_binding(),
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

            mask_buffer = grown_mask_buffer;
        }

        match bucket_params.aa_approach {
            BucketAntialiasApproach::None => {}
            BucketAntialiasApproach::Fxaa => {
                let fxaa_params = FxaaParams::default();
                let mut fxaa_params_buffer =
                    DynamicBuffer::new(Some("fxaa_params_buffer"), BufferUsages::UNIFORM);
                fxaa_params_buffer.push(&FxaaParamsInner {
                    edge_threshold_min: fxaa_params.edge_threshold_min,
                    edge_threshold_max: fxaa_params.edge_threshold_max,
                    iterations: fxaa_params.iterations,
                    subpixel_quality: fxaa_params.subpixel_quality,
                    image_size: bucket_params.image_size,
                });
                fxaa_params_buffer.write_buffer(device, queue);

                let smoothed_mask_buffer = device.create_buffer(&BufferDescriptor {
                    label: Some("smoothed_mask_buffer"),
                    size: mask_buffer_size(mask_tile_indices.len() as u32),
                    usage: BufferUsages::STORAGE,
                    mapped_at_creation: false,
                });

                let fxaa_bind_group = device.create_bind_group(&BindGroupDescriptor {
                    label: Some("fxaa_bind_group"),
                    layout: &self.fxaa_layout,
                    entries: &[
                        BindGroupEntry {
                            binding: 0,
                            resource: mask_buffer.as_entire_binding(),
                        },
                        BindGroupEntry {
                            binding: 1,
                            resource: smoothed_mask_buffer.as_entire_binding(),
                        },
                        BindGroupEntry {
                            binding: 2,
                            resource: mask_tile_info_buffer.as_entire_binding(),
                        },
                        BindGroupEntry {
                            binding: 3,
                            resource: fxaa_params_buffer.binding().unwrap(),
                        },
                    ],
                });

                let mut ec = device.create_command_encoder(&Default::default());
                {
                    let mut pass = ec.begin_compute_pass(&ComputePassDescriptor {
                        label: Some("smaa_pass"),
                        timestamp_writes: None,
                    });
                    pass.set_pipeline(&self.fxaa_pipeline);
                    pass.set_bind_group(0, &fxaa_bind_group, &[]);
                    pass.dispatch_workgroups(
                        dispatch_xy,
                        dispatch_xy,
                        mask_tile_indices.len() as u32,
                    );
                }
                queue.submit([ec.finish()]);

                mask_buffer = smoothed_mask_buffer;
            }
            BucketAntialiasApproach::Feather(radius) => 'a: {
                if radius == 0 {
                    break 'a;
                }

                let max_jump = radius.next_power_of_two();
                let (jump_params_buffer, jump_params_offsets) =
                    create_jfa_params(device, queue, max_jump, "feather_jump_params");
                let jump_iterations = jump_params_offsets.len();

                let common_entries = vec![
                    BindGroupEntry {
                        binding: 0,
                        resource: bucket_params_buffer.binding().unwrap(),
                    },
                    BindGroupEntry {
                        binding: 1,
                        resource: mask_buffer.as_entire_binding(),
                    },
                    BindGroupEntry {
                        binding: 4,
                        resource: mask_tile_info_buffer.as_entire_binding(),
                    },
                    BindGroupEntry {
                        binding: 5,
                        resource: jump_params_buffer.binding().unwrap(),
                    },
                ];

                let bind_groups = {
                    let a_to_b_entries = common_entries
                        .clone()
                        .into_iter()
                        .chain([
                            BindGroupEntry {
                                binding: 2,
                                resource: BindingResource::TextureView(&seed_texture_a_view),
                            },
                            BindGroupEntry {
                                binding: 3,
                                resource: BindingResource::TextureView(&seed_texture_b_view),
                            },
                        ])
                        .collect::<Vec<_>>();
                    let b_to_a_entries = common_entries
                        .into_iter()
                        .chain([
                            BindGroupEntry {
                                binding: 2,
                                resource: BindingResource::TextureView(&seed_texture_b_view),
                            },
                            BindGroupEntry {
                                binding: 3,
                                resource: BindingResource::TextureView(&seed_texture_a_view),
                            },
                        ])
                        .collect::<Vec<_>>();

                    let a_to_b_group = device.create_bind_group(&BindGroupDescriptor {
                        label: Some("feather_seed_a_to_b_bind_group"),
                        layout: &self.close_gap_and_feather_layout,
                        entries: &a_to_b_entries,
                    });
                    let b_to_a_group = device.create_bind_group(&BindGroupDescriptor {
                        label: Some("feather_seed_b_to_a_bind_group"),
                        layout: &self.close_gap_and_feather_layout,
                        entries: &b_to_a_entries,
                    });

                    [a_to_b_group, b_to_a_group]
                };

                let mut ec = device.create_command_encoder(&Default::default());
                {
                    let mut pass = ec.begin_compute_pass(&ComputePassDescriptor {
                        label: Some("feather_seed_edges_pass"),
                        timestamp_writes: None,
                    });
                    pass.set_pipeline(&self.close_gap_and_feather_seed_pipeline);
                    pass.set_bind_group(0, &bind_groups[1], &[jump_params_offsets[0]]);
                    pass.dispatch_workgroups(
                        dispatch_xy,
                        dispatch_xy,
                        mask_tile_indices.len() as u32,
                    );
                }

                {
                    let mut pass = ec.begin_compute_pass(&ComputePassDescriptor {
                        label: Some("feather_jfa_pass"),
                        timestamp_writes: None,
                    });
                    pass.set_pipeline(&self.close_gap_and_feather_jump_pipeline);
                    for i in 0..jump_iterations {
                        pass.set_bind_group(0, &bind_groups[i % 2], &[jump_params_offsets[i]]);
                        pass.dispatch_workgroups(
                            dispatch_xy,
                            dispatch_xy,
                            mask_tile_indices.len() as u32,
                        );
                    }
                }

                {
                    let mut pass = ec.begin_compute_pass(&ComputePassDescriptor {
                        label: Some("feather_resolve_pass"),
                        timestamp_writes: None,
                    });
                    pass.set_pipeline(&self.feather_resolve_pipeline);
                    pass.set_bind_group(
                        0,
                        &bind_groups[jump_iterations % 2],
                        &[jump_params_offsets[jump_iterations - 1]],
                    );
                    pass.dispatch_workgroups(
                        dispatch_xy,
                        dispatch_xy,
                        mask_tile_indices.len() as u32,
                    );
                }
                queue.submit([ec.finish()]);
            }
        };

        let filled_tiles = self.get_unempty_tiles(
            device,
            queue,
            &mask_buffer,
            &mask_tile_info_buffer,
            &mask_tile_indices,
        );
        for index in &filled_tiles {
            output_layer.get_tile_or_allocate(*index);
        }

        let mut ec = device.create_command_encoder(&Default::default());

        let (Some(output_texture), Some(output_tile_info)) =
            (output_layer.texture(), output_layer.tile_info_buffer())
        else {
            return Vec::new();
        };

        let composite_bind_group = device.create_bind_group(&BindGroupDescriptor {
            label: Some("composite_bind_group"),
            layout: &self.composite_layout,
            entries: &[
                BindGroupEntry {
                    binding: 0,
                    resource: bucket_params_buffer.binding().unwrap(),
                },
                BindGroupEntry {
                    binding: 1,
                    resource: mask_buffer.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 2,
                    resource: mask_tile_info_buffer.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 3,
                    resource: BindingResource::TextureView(output_texture),
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

        queue.submit([ec.finish()]);

        unsafe { device.stop_graphics_debugger_capture() };

        info!("Filled {} tiles: {:?}", filled_tiles.len(), filled_tiles);

        filled_tiles
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

    fn get_unempty_tiles(
        &self,
        device: &Device,
        queue: &Queue,
        mask_buffer: &Buffer,
        mask_tile_info_buffer: &Buffer,
        mask_tile_indices: &IndexSet<IVec2>,
    ) -> Vec<IVec2> {
        let mut ec = device.create_command_encoder(&Default::default());

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
                    resource: mask_buffer.as_entire_binding(),
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
            let mut pass = ec.begin_compute_pass(&ComputePassDescriptor {
                label: Some("scan_pixels_pass"),
                ..Default::default()
            });
            pass.set_pipeline(&self.scan_pixels_pipeline);
            pass.set_bind_group(0, &scan_pixels_bind_group, &[]);
            pass.dispatch_workgroups(
                GpuTileStorageInner::TILE_SIZE.div_ceil(16),
                GpuTileStorageInner::TILE_SIZE.div_ceil(16),
                mask_tile_indices.len() as u32,
            );
        }

        let is_not_empty_readback =
            create_readback_buffer_and_schedule_copy(device, &mut ec, &is_not_empty_buffer);
        let is_not_empty_readback_async =
            readback_buffer_on_submit_async::<Vec<u32>, _>(&mut ec, &is_not_empty_readback, ..);

        let si = queue.submit([ec.finish()]);
        device.poll_indefinitely_for(si).unwrap();

        let is_not_empty = is_not_empty_readback_async.block_on().unwrap();
        mask_tile_indices
            .iter()
            .zip(is_not_empty)
            .filter_map(|(i, is_not_empty)| if is_not_empty == 1 { Some(*i) } else { None })
            .collect::<Vec<_>>()
    }
}

fn mask_buffer_size(tile_count: u32) -> u64 {
    (GpuTileStorageInner::TILE_SIZE * GpuTileStorageInner::TILE_SIZE * tile_count / 4 * 8) as u64
}

fn create_jfa_params(
    device: &Device,
    queue: &Queue,
    max_jump: u32,
    label: &'static str,
) -> (DynamicBuffer<JumpParams>, Vec<u32>) {
    let mut jump_params_buffer = DynamicBuffer::new(Some(label), BufferUsages::UNIFORM);
    let mut jump_params_offsets = Vec::new();
    let mut jump = max_jump.max(1);
    while jump > 0 {
        let offset = jump_params_buffer.push(&JumpParams { jump });
        jump_params_offsets.push(offset as u32);
        jump /= 2;
    }
    jump_params_buffer.write_buffer(device, queue);

    (jump_params_buffer, jump_params_offsets)
}
