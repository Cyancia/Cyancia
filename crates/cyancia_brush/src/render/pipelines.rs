use std::{
    borrow::Cow,
    num::{NonZeroU32, NonZeroU64},
};

use bevy_math::URect;
use cyancia_image::tile::{GpuTileInfo, GpuTileStorageInner};
use cyancia_render::buffer::DynamicBuffer;
use encase::ShaderType;
use glam::UVec3;
use toml::de;
use wesl::{VirtualResolver, Wesl};
use wgpu::{
    BindGroup, BindGroupDescriptor, BindGroupEntry, BindGroupLayoutDescriptor,
    BindGroupLayoutEntry, BindingResource, BindingType, Buffer, BufferBindingType, BufferUsages,
    CommandEncoder, ComputePassDescriptor, ComputePipeline, ComputePipelineDescriptor, Device,
    PipelineLayoutDescriptor, Queue, ShaderModuleDescriptor, ShaderSource, ShaderStages,
    StorageTextureAccess, TextureSampleType, TextureViewDimension,
};

use crate::render::{
    DabInfos, EXTERNAL_VARIABLE_BASE_BINDING, OutputSamples, PassFence, PenInput, PenInputSampler,
    StrokeInfo, StrokeResources, dynamic_intermediate_buffer::DynamicGpuTileInfoBuffer,
};

pub struct BrushInputSamplingPipeline {
    bind_group: BindGroup,
    pipeline: ComputePipeline,
}

impl BrushInputSamplingPipeline {
    pub fn new(
        device: &Device,
        resources: &StrokeResources,
        compiled_shader: Cow<'_, str>,
    ) -> Self {
        let layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some("brush input sampling layout"),
            entries: &[
                BindGroupLayoutEntry {
                    binding: 0,
                    visibility: ShaderStages::COMPUTE,
                    ty: BindingType::Buffer {
                        ty: BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: Some(PenInput::min_size()),
                    },
                    count: None,
                },
                BindGroupLayoutEntry {
                    binding: 1,
                    visibility: ShaderStages::COMPUTE,
                    ty: BindingType::Buffer {
                        ty: BufferBindingType::Storage { read_only: false },
                        has_dynamic_offset: false,
                        min_binding_size: Some(PenInputSampler::min_size()),
                    },
                    count: None,
                },
                BindGroupLayoutEntry {
                    binding: 2,
                    visibility: ShaderStages::COMPUTE,
                    ty: BindingType::Buffer {
                        ty: BufferBindingType::Storage { read_only: false },
                        has_dynamic_offset: false,
                        min_binding_size: Some(OutputSamples::min_size()),
                    },
                    count: None,
                },
                BindGroupLayoutEntry {
                    binding: 3,
                    visibility: ShaderStages::COMPUTE,
                    ty: BindingType::Buffer {
                        ty: BufferBindingType::Storage { read_only: false },
                        has_dynamic_offset: false,
                        min_binding_size: Some(UVec3::min_size()),
                    },
                    count: None,
                },
                BindGroupLayoutEntry {
                    binding: 4,
                    visibility: ShaderStages::COMPUTE,
                    ty: BindingType::Buffer {
                        ty: BufferBindingType::Storage { read_only: false },
                        has_dynamic_offset: false,
                        min_binding_size: Some(StrokeInfo::min_size()),
                    },
                    count: None,
                },
            ],
        });

        let bind_group = device.create_bind_group(&BindGroupDescriptor {
            label: Some("brush input sampling bind group"),
            layout: &layout,
            entries: &[
                BindGroupEntry {
                    binding: 0,
                    resource: resources.pen_input.binding().unwrap(),
                },
                BindGroupEntry {
                    binding: 1,
                    resource: resources.input_sampler.binding().unwrap(),
                },
                BindGroupEntry {
                    binding: 2,
                    resource: resources.output_samples.binding().unwrap(),
                },
                BindGroupEntry {
                    binding: 3,
                    resource: resources.estimate_dispatch.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 4,
                    resource: resources.stroke_info.binding().unwrap(),
                },
            ],
        });

        let shader = device.create_shader_module(ShaderModuleDescriptor {
            label: Some("brush input sampling shader"),
            source: ShaderSource::Wgsl(compiled_shader),
        });

        let pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: Some("brush input sampling pipeline layout"),
            bind_group_layouts: &[&layout],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_compute_pipeline(&ComputePipelineDescriptor {
            label: Some("brush input sampling pipeline"),
            layout: Some(&pipeline_layout),
            module: &shader,
            entry_point: Some("main"),
            compilation_options: Default::default(),
            cache: None,
        });

        Self {
            bind_group,
            pipeline,
        }
    }

    pub fn dispatch(&self, ec: &mut CommandEncoder) {
        {
            ec.push_debug_group("brush preset input sampling");
            let mut pass = ec.begin_compute_pass(&ComputePassDescriptor {
                label: Some("brush sample compute pass"),
                ..Default::default()
            });

            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &self.bind_group, &[]);
            pass.dispatch_workgroups(1, 1, 1);
        }
        ec.pop_debug_group();
    }
}

pub struct BrushTileAllocationPipeline {
    bind_group: BindGroup,
    pipeline: ComputePipeline,
}

impl BrushTileAllocationPipeline {
    pub fn new(device: &Device, resources: &StrokeResources, is_postprocess: bool) -> Self {
        let layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some("brush tile allocation layout"),
            entries: &[
                BindGroupLayoutEntry {
                    binding: 0,
                    visibility: ShaderStages::COMPUTE,
                    ty: BindingType::Buffer {
                        ty: BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: Some(DabInfos::min_size()),
                    },
                    count: None,
                },
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
                BindGroupLayoutEntry {
                    binding: 2,
                    visibility: ShaderStages::COMPUTE,
                    ty: BindingType::Buffer {
                        ty: BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: Some(StrokeInfo::min_size()),
                    },
                    count: None,
                },
            ],
        });

        let bind_group = device.create_bind_group(&BindGroupDescriptor {
            label: Some("brush tile allocation bind group"),
            layout: &layout,
            entries: &[
                BindGroupEntry {
                    binding: 0,
                    resource: resources.dab_infos.binding().unwrap(),
                },
                BindGroupEntry {
                    binding: 1,
                    resource: resources
                        .intermediate_buffers
                        .tile_info_buffer()
                        .as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 2,
                    resource: resources.stroke_info.binding().unwrap(),
                },
            ],
        });

        let mut resolver = VirtualResolver::default();
        resolver.add_module(
            "package::brush_tile_allocation".parse().unwrap(),
            include_str!("brush_tile_allocation.wesl").into(),
        );
        resolver.add_module(
            "package::brush::brush_types".parse().unwrap(),
            include_str!("brush_types.wesl").into(),
        );
        resolver.add_module(
            "package::image::image_tiling".parse().unwrap(),
            include_str!("../../../cyancia_image/src/shaders/image_tiling.wesl").into(),
        );
        let mut compiler = Wesl::new_barebones().set_custom_resolver(resolver);
        compiler.set_mangler(Default::default());
        compiler.set_options(Default::default());
        compiler.set_feature("TILE_ALLOCATION", true);
        compiler.set_feature("POSTPROCESS", is_postprocess);
        let shader_code = compiler
            .compile(&"package::brush_tile_allocation".parse().unwrap())
            .unwrap()
            .to_string();

        let shader = device.create_shader_module(ShaderModuleDescriptor {
            label: Some("brush tile allocation shader"),
            source: ShaderSource::Wgsl(shader_code.into()),
        });

        let pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: Some("brush tile allocation pipeline layout"),
            bind_group_layouts: &[&layout],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_compute_pipeline(&ComputePipelineDescriptor {
            label: Some("brush tile allocation pipeline"),
            layout: Some(&pipeline_layout),
            module: &shader,
            entry_point: Some("allocate"),
            compilation_options: Default::default(),
            cache: None,
        });

        Self {
            bind_group,
            pipeline,
        }
    }

    pub fn dispatch(&self, ec: &mut CommandEncoder, resources: &StrokeResources) {
        ec.push_debug_group("brush preset tile allocation");
        {
            let mut pass = ec.begin_compute_pass(&ComputePassDescriptor {
                label: Some("brush tile allocation compute pass"),
                ..Default::default()
            });

            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &self.bind_group, &[]);
            pass.dispatch_workgroups_indirect(&resources.tile_allocation_dispatch, 0);
        }
        ec.pop_debug_group();
    }
}

pub struct BrushEstimatePipeline {
    bind_group: BindGroup,
    pipeline: ComputePipeline,
}

impl BrushEstimatePipeline {
    pub fn new(
        device: &Device,
        resources: &StrokeResources,
        compiled_shader: Cow<'_, str>,
    ) -> Self {
        let layout_entries = {
            let mut entries = vec![
                BindGroupLayoutEntry {
                    binding: 0,
                    visibility: ShaderStages::COMPUTE,
                    ty: BindingType::Buffer {
                        ty: BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: Some(OutputSamples::min_size()),
                    },
                    count: None,
                },
                BindGroupLayoutEntry {
                    binding: 1,
                    visibility: ShaderStages::COMPUTE,
                    ty: BindingType::Buffer {
                        ty: BufferBindingType::Storage { read_only: false },
                        has_dynamic_offset: false,
                        min_binding_size: Some(StrokeInfo::min_size()),
                    },
                    count: None,
                },
                BindGroupLayoutEntry {
                    binding: 2,
                    visibility: ShaderStages::COMPUTE,
                    ty: BindingType::Texture {
                        sample_type: TextureSampleType::Float { filterable: false },
                        view_dimension: TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                BindGroupLayoutEntry {
                    binding: 3,
                    visibility: ShaderStages::COMPUTE,
                    ty: BindingType::Buffer {
                        ty: BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: Some(URect::min_size()),
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
                    ty: BindingType::StorageTexture {
                        access: StorageTextureAccess::ReadOnly,
                        format: resources.target_layer.texture().format(),
                        view_dimension: TextureViewDimension::D2Array,
                    },
                    count: None,
                },
                BindGroupLayoutEntry {
                    binding: 6,
                    visibility: ShaderStages::COMPUTE,
                    ty: BindingType::Buffer {
                        ty: BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: Some(DynamicGpuTileInfoBuffer::min_size()),
                    },
                    count: None,
                },
                BindGroupLayoutEntry {
                    binding: 7,
                    visibility: ShaderStages::COMPUTE,
                    ty: BindingType::StorageTexture {
                        access: StorageTextureAccess::ReadWrite,
                        format: resources.intermediate_buffers.textures()[0]
                            .texture()
                            .format(),
                        view_dimension: TextureViewDimension::D2Array,
                    },
                    count: None,
                },
                BindGroupLayoutEntry {
                    binding: 8,
                    visibility: ShaderStages::COMPUTE,
                    ty: BindingType::StorageTexture {
                        access: StorageTextureAccess::ReadWrite,
                        format: resources.intermediate_buffers.textures()[1]
                            .texture()
                            .format(),
                        view_dimension: TextureViewDimension::D2Array,
                    },
                    count: None,
                },
                BindGroupLayoutEntry {
                    binding: 9,
                    visibility: ShaderStages::COMPUTE,
                    ty: BindingType::Buffer {
                        ty: BufferBindingType::Storage { read_only: false },
                        has_dynamic_offset: false,
                        min_binding_size: Some(DabInfos::min_size()),
                    },
                    count: None,
                },
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
            entries.extend(resources.external_var_layouts.clone());
            entries
        };

        let layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some("brush estimate layout"),
            entries: &layout_entries,
        });

        let bind_group_entries = {
            let mut entries = vec![
                BindGroupEntry {
                    binding: 0,
                    resource: resources.output_samples.binding().unwrap(),
                },
                BindGroupEntry {
                    binding: 1,
                    resource: resources.stroke_info.binding().unwrap(),
                },
                BindGroupEntry {
                    binding: 2,
                    resource: BindingResource::TextureView(
                        resources.referenced_textures.texture_view(),
                    ),
                },
                BindGroupEntry {
                    binding: 3,
                    resource: resources.referenced_textures.atlas_bounds_buffer_binding(),
                },
                BindGroupEntry {
                    binding: 4,
                    resource: resources.target_layer_tile_info.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 5,
                    resource: BindingResource::TextureView(&resources.target_layer),
                },
                BindGroupEntry {
                    binding: 6,
                    resource: resources
                        .intermediate_buffers
                        .tile_info_buffer()
                        .as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 7,
                    resource: BindingResource::TextureView(
                        &resources.intermediate_buffers.textures()[0],
                    ),
                },
                BindGroupEntry {
                    binding: 8,
                    resource: BindingResource::TextureView(
                        &resources.intermediate_buffers.textures()[1],
                    ),
                },
                BindGroupEntry {
                    binding: 9,
                    resource: resources.dab_infos.binding().unwrap(),
                },
                BindGroupEntry {
                    binding: 16,
                    resource: resources.tile_allocation_dispatch.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 17,
                    resource: resources.main_dispatch.as_entire_binding(),
                },
            ];
            entries.extend(resources.external_var_bindings());
            entries
        };
        let bind_group = device.create_bind_group(&BindGroupDescriptor {
            label: Some("brush estimate bind group"),
            layout: &layout,
            entries: &bind_group_entries,
        });

        let shader = device.create_shader_module(ShaderModuleDescriptor {
            label: Some("brush estimate shader"),
            source: ShaderSource::Wgsl(compiled_shader),
        });

        let pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: Some("brush estimate pipeline layout"),
            bind_group_layouts: &[&layout],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_compute_pipeline(&ComputePipelineDescriptor {
            label: Some("brush estimate pipeline"),
            layout: Some(&pipeline_layout),
            module: &shader,
            entry_point: Some("estimate"),
            compilation_options: Default::default(),
            cache: None,
        });

        Self {
            bind_group,
            pipeline,
        }
    }

    pub fn dispatch(&self, ec: &mut CommandEncoder, x: u32, y: u32, z: u32) {
        ec.push_debug_group("brush preset estimate");
        {
            let mut pass = ec.begin_compute_pass(&ComputePassDescriptor {
                label: Some("brush estimate compute pass"),
                ..Default::default()
            });

            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &self.bind_group, &[]);
            pass.dispatch_workgroups(x, y, z);
        }
        ec.pop_debug_group();
    }

    pub fn dispatch_indirect(&self, ec: &mut CommandEncoder, resources: &StrokeResources) {
        ec.push_debug_group("brush preset estimate");
        {
            let mut pass = ec.begin_compute_pass(&ComputePassDescriptor {
                label: Some("brush estimate compute pass"),
                ..Default::default()
            });

            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &self.bind_group, &[]);
            pass.dispatch_workgroups_indirect(&resources.estimate_dispatch, 0);
        }
        ec.pop_debug_group();
    }
}

pub struct BrushMainPipeline {
    bind_group: BindGroup,
    pipeline: ComputePipeline,
}

impl BrushMainPipeline {
    pub fn new(
        device: &Device,
        resources: &StrokeResources,
        compiled_shader: Cow<'_, str>,
    ) -> Self {
        let layout_entries = {
            let mut entries = vec![
                BindGroupLayoutEntry {
                    binding: 0,
                    visibility: ShaderStages::COMPUTE,
                    ty: BindingType::Buffer {
                        ty: BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: Some(OutputSamples::min_size()),
                    },
                    count: None,
                },
                BindGroupLayoutEntry {
                    binding: 1,
                    visibility: ShaderStages::COMPUTE,
                    ty: BindingType::Buffer {
                        ty: BufferBindingType::Storage { read_only: false },
                        has_dynamic_offset: false,
                        min_binding_size: Some(StrokeInfo::min_size()),
                    },
                    count: None,
                },
                BindGroupLayoutEntry {
                    binding: 2,
                    visibility: ShaderStages::COMPUTE,
                    ty: BindingType::Texture {
                        sample_type: TextureSampleType::Float { filterable: false },
                        view_dimension: TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                BindGroupLayoutEntry {
                    binding: 3,
                    visibility: ShaderStages::COMPUTE,
                    ty: BindingType::Buffer {
                        ty: BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: Some(URect::min_size()),
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
                    ty: BindingType::StorageTexture {
                        access: StorageTextureAccess::ReadOnly,
                        format: resources.target_layer.texture().format(),
                        view_dimension: TextureViewDimension::D2Array,
                    },
                    count: None,
                },
                BindGroupLayoutEntry {
                    binding: 6,
                    visibility: ShaderStages::COMPUTE,
                    ty: BindingType::Buffer {
                        ty: BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: Some(DynamicGpuTileInfoBuffer::min_size()),
                    },
                    count: None,
                },
                BindGroupLayoutEntry {
                    binding: 7,
                    visibility: ShaderStages::COMPUTE,
                    ty: BindingType::StorageTexture {
                        access: StorageTextureAccess::ReadWrite,
                        format: resources.intermediate_buffers.textures()[0]
                            .texture()
                            .format(),
                        view_dimension: TextureViewDimension::D2Array,
                    },
                    count: None,
                },
                BindGroupLayoutEntry {
                    binding: 8,
                    visibility: ShaderStages::COMPUTE,
                    ty: BindingType::StorageTexture {
                        access: StorageTextureAccess::ReadWrite,
                        format: resources.intermediate_buffers.textures()[1]
                            .texture()
                            .format(),
                        view_dimension: TextureViewDimension::D2Array,
                    },
                    count: None,
                },
                BindGroupLayoutEntry {
                    binding: 9,
                    visibility: ShaderStages::COMPUTE,
                    ty: BindingType::Buffer {
                        ty: BufferBindingType::Storage { read_only: false },
                        has_dynamic_offset: false,
                        min_binding_size: Some(DabInfos::min_size()),
                    },
                    count: None,
                },
                BindGroupLayoutEntry {
                    binding: 10,
                    visibility: ShaderStages::COMPUTE,
                    ty: BindingType::Buffer {
                        ty: BufferBindingType::Storage { read_only: false },
                        has_dynamic_offset: false,
                        min_binding_size: Some(PassFence::min_size()),
                    },
                    count: None,
                },
            ];
            entries.extend(resources.external_var_layouts.clone());
            entries
        };

        let layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some("brush main layout"),
            entries: &layout_entries,
        });

        let bind_group_entries = {
            let mut entries = vec![
                BindGroupEntry {
                    binding: 0,
                    resource: resources.output_samples.binding().unwrap(),
                },
                BindGroupEntry {
                    binding: 1,
                    resource: resources.stroke_info.binding().unwrap(),
                },
                BindGroupEntry {
                    binding: 2,
                    resource: BindingResource::TextureView(
                        resources.referenced_textures.texture_view(),
                    ),
                },
                BindGroupEntry {
                    binding: 3,
                    resource: resources.referenced_textures.atlas_bounds_buffer_binding(),
                },
                BindGroupEntry {
                    binding: 4,
                    resource: resources.target_layer_tile_info.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 5,
                    resource: BindingResource::TextureView(&resources.target_layer),
                },
                BindGroupEntry {
                    binding: 6,
                    resource: resources
                        .intermediate_buffers
                        .tile_info_buffer()
                        .as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 7,
                    resource: BindingResource::TextureView(
                        &resources.intermediate_buffers.textures()[0],
                    ),
                },
                BindGroupEntry {
                    binding: 8,
                    resource: BindingResource::TextureView(
                        &resources.intermediate_buffers.textures()[1],
                    ),
                },
                BindGroupEntry {
                    binding: 9,
                    resource: resources.dab_infos.binding().unwrap(),
                },
                BindGroupEntry {
                    binding: 10,
                    resource: resources.pass_fence.binding().unwrap(),
                },
            ];
            entries.extend(resources.external_var_bindings());
            entries
        };
        let bind_group = device.create_bind_group(&BindGroupDescriptor {
            label: Some("brush main bind group"),
            layout: &layout,
            entries: &bind_group_entries,
        });

        let shader = device.create_shader_module(ShaderModuleDescriptor {
            label: Some("brush main shader"),
            source: ShaderSource::Wgsl(compiled_shader),
        });

        let pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: Some("brush main pipeline layout"),
            bind_group_layouts: &[&layout],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_compute_pipeline(&ComputePipelineDescriptor {
            label: Some("brush main pipeline"),
            layout: Some(&pipeline_layout),
            module: &shader,
            entry_point: Some("main"),
            compilation_options: Default::default(),
            cache: None,
        });

        Self {
            bind_group,
            pipeline,
        }
    }

    pub fn dispatch(&self, ec: &mut CommandEncoder, resources: &StrokeResources) {
        ec.push_debug_group("brush preset main");
        {
            let mut pass = ec.begin_compute_pass(&ComputePassDescriptor {
                label: Some("brush main compute pass"),
                ..Default::default()
            });

            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &self.bind_group, &[]);
            pass.dispatch_workgroups_indirect(&resources.main_dispatch, 0);
        }
        ec.pop_debug_group();
    }
}
