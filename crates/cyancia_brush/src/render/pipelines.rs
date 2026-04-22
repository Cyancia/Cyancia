use std::borrow::Cow;

use bevy_math::URect;
use cyancia_image::{
    dynamic_intermediate_buffer::{DynamicGpuTileInfoBuffer, DynamicIntermediateBuffer},
    tile::GpuTileInfo,
};
use cyancia_render::{buffer::DynamicBuffer, texture_atlas::TextureAtlas};
use encase::ShaderType;
use glam::UVec3;
use wesl::{VirtualResolver, Wesl};
use wgpu::{
    BindGroup, BindGroupDescriptor, BindGroupEntry, BindGroupLayout, BindGroupLayoutDescriptor,
    BindGroupLayoutEntry, BindingResource, BindingType, Buffer, BufferBindingType, CommandEncoder,
    ComputePassDescriptor, ComputePipeline, ComputePipelineDescriptor, Device,
    PipelineLayoutDescriptor, ShaderModuleDescriptor, ShaderSource, ShaderStages,
    StorageTextureAccess, TextureFormat, TextureSampleType, TextureView, TextureViewDimension,
};

use crate::render::{
    DabInfos, EXTERNAL_VARIABLE_BASE_BINDING, OutputSamples, PassFence, PenInput, PenInputSampler,
    StrokeInfo,
};

fn external_var_entries(buffers: &[Buffer]) -> Vec<BindGroupEntry<'_>> {
    buffers
        .iter()
        .enumerate()
        .map(|(i, buffer)| BindGroupEntry {
            binding: EXTERNAL_VARIABLE_BASE_BINDING + i as u32,
            resource: BindingResource::Buffer(buffer.as_entire_buffer_binding()),
        })
        .collect()
}

#[derive(Debug, Clone)]
pub struct BrushInputSamplingPipeline {
    bind_group: Option<BindGroup>,
    layout: BindGroupLayout,
    pipeline: ComputePipeline,
}

impl BrushInputSamplingPipeline {
    pub fn new(device: &Device, compiled_shader: Cow<'_, str>) -> Self {
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
            bind_group: None,
            layout,
            pipeline,
        }
    }

    pub fn prepare(
        &mut self,
        device: &Device,
        pen_input: &DynamicBuffer<PenInput>,
        input_sampler: &DynamicBuffer<PenInputSampler>,
        output_samples: &DynamicBuffer<OutputSamples>,
        estimate_dispatch: &Buffer,
        stroke_info: &DynamicBuffer<StrokeInfo>,
    ) {
        self.bind_group = Some(device.create_bind_group(&BindGroupDescriptor {
            label: Some("brush input sampling bind group"),
            layout: &self.layout,
            entries: &[
                BindGroupEntry {
                    binding: 0,
                    resource: pen_input.binding().unwrap(),
                },
                BindGroupEntry {
                    binding: 1,
                    resource: input_sampler.binding().unwrap(),
                },
                BindGroupEntry {
                    binding: 2,
                    resource: output_samples.binding().unwrap(),
                },
                BindGroupEntry {
                    binding: 3,
                    resource: estimate_dispatch.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 4,
                    resource: stroke_info.binding().unwrap(),
                },
            ],
        }));
    }

    pub fn dispatch(&self, ec: &mut CommandEncoder) {
        let bind_group = self
            .bind_group
            .as_ref()
            .expect("BrushInputSamplingPipeline::prepare() must be called before dispatch()");

        ec.push_debug_group("brush preset input sampling");
        {
            let mut pass = ec.begin_compute_pass(&ComputePassDescriptor {
                label: Some("brush sample compute pass"),
                ..Default::default()
            });

            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, bind_group, &[]);
            pass.dispatch_workgroups(1, 1, 1);
        }
        ec.pop_debug_group();
    }
}

#[derive(Debug, Clone)]
pub struct BrushTileAllocationPipeline {
    bind_group: Option<BindGroup>,
    layout: BindGroupLayout,
    pipeline: ComputePipeline,
}

impl BrushTileAllocationPipeline {
    pub fn new(device: &Device, is_postprocess: bool) -> Self {
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
            bind_group: None,
            layout,
            pipeline,
        }
    }

    pub fn prepare(
        &mut self,
        device: &Device,
        dab_infos: &DynamicBuffer<DabInfos>,
        tile_info_buffer: &Buffer,
        stroke_info: &DynamicBuffer<StrokeInfo>,
    ) {
        self.bind_group = Some(device.create_bind_group(&BindGroupDescriptor {
            label: Some("brush tile allocation bind group"),
            layout: &self.layout,
            entries: &[
                BindGroupEntry {
                    binding: 0,
                    resource: dab_infos.binding().unwrap(),
                },
                BindGroupEntry {
                    binding: 1,
                    resource: tile_info_buffer.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 2,
                    resource: stroke_info.binding().unwrap(),
                },
            ],
        }));
    }

    pub fn dispatch(&self, ec: &mut CommandEncoder, tile_allocation_dispatch: &Buffer) {
        let bind_group = self
            .bind_group
            .as_ref()
            .expect("BrushTileAllocationPipeline::prepare() must be called before dispatch()");

        ec.push_debug_group("brush preset tile allocation");
        {
            let mut pass = ec.begin_compute_pass(&ComputePassDescriptor {
                label: Some("brush tile allocation compute pass"),
                ..Default::default()
            });

            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, bind_group, &[]);
            pass.dispatch_workgroups_indirect(tile_allocation_dispatch, 0);
        }
        ec.pop_debug_group();
    }
}

#[derive(Debug, Clone)]
pub struct BrushEstimatePipeline {
    bind_group: Option<BindGroup>,
    layout: BindGroupLayout,
    pipeline: ComputePipeline,
}

impl BrushEstimatePipeline {
    pub fn new(
        device: &Device,
        compiled_shader: Cow<'_, str>,
        target_layer_format: TextureFormat,
        external_var_layouts: &[BindGroupLayoutEntry],
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
                        format: target_layer_format,
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
            entries.extend_from_slice(external_var_layouts);
            entries
        };

        let layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some("brush estimate layout"),
            entries: &layout_entries,
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
            bind_group: None,
            layout,
            pipeline,
        }
    }

    pub fn prepare(
        &mut self,
        device: &Device,
        output_samples: &DynamicBuffer<OutputSamples>,
        stroke_info: &DynamicBuffer<StrokeInfo>,
        referenced_textures: &TextureAtlas,
        target_layer_tile_info: &Buffer,
        target_layer: &TextureView,
        dab_infos: &DynamicBuffer<DabInfos>,
        tile_allocation_dispatch: &Buffer,
        main_dispatch: &Buffer,
        external_var_buffers: &[Buffer],
    ) {
        let mut entries = vec![
            BindGroupEntry {
                binding: 0,
                resource: output_samples.binding().unwrap(),
            },
            BindGroupEntry {
                binding: 1,
                resource: stroke_info.binding().unwrap(),
            },
            BindGroupEntry {
                binding: 2,
                resource: BindingResource::TextureView(referenced_textures.texture_view()),
            },
            BindGroupEntry {
                binding: 3,
                resource: referenced_textures.atlas_bounds_buffer_binding(),
            },
            BindGroupEntry {
                binding: 4,
                resource: target_layer_tile_info.as_entire_binding(),
            },
            BindGroupEntry {
                binding: 5,
                resource: BindingResource::TextureView(target_layer),
            },
            BindGroupEntry {
                binding: 9,
                resource: dab_infos.binding().unwrap(),
            },
            BindGroupEntry {
                binding: 16,
                resource: tile_allocation_dispatch.as_entire_binding(),
            },
            BindGroupEntry {
                binding: 17,
                resource: main_dispatch.as_entire_binding(),
            },
        ];
        entries.extend(external_var_entries(external_var_buffers));

        self.bind_group = Some(device.create_bind_group(&BindGroupDescriptor {
            label: Some("brush estimate bind group"),
            layout: &self.layout,
            entries: &entries,
        }));
    }

    pub fn dispatch(&self, ec: &mut CommandEncoder, x: u32, y: u32, z: u32) {
        let bind_group = self
            .bind_group
            .as_ref()
            .expect("BrushEstimatePipeline::prepare() must be called before dispatch()");

        ec.push_debug_group("brush preset estimate");
        {
            let mut pass = ec.begin_compute_pass(&ComputePassDescriptor {
                label: Some("brush estimate compute pass"),
                ..Default::default()
            });

            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, bind_group, &[]);
            pass.dispatch_workgroups(x, y, z);
        }
        ec.pop_debug_group();
    }

    pub fn dispatch_indirect(&self, ec: &mut CommandEncoder, estimate_dispatch: &Buffer) {
        let bind_group = self
            .bind_group
            .as_ref()
            .expect("BrushEstimatePipeline::prepare() must be called before dispatch_indirect()");

        ec.push_debug_group("brush preset estimate");
        {
            let mut pass = ec.begin_compute_pass(&ComputePassDescriptor {
                label: Some("brush estimate compute pass"),
                ..Default::default()
            });

            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, bind_group, &[]);
            pass.dispatch_workgroups_indirect(estimate_dispatch, 0);
        }
        ec.pop_debug_group();
    }
}

#[derive(Debug, Clone)]
pub struct BrushMainPipeline {
    bind_group: Option<BindGroup>,
    layout: BindGroupLayout,
    pipeline: ComputePipeline,
}

impl BrushMainPipeline {
    pub fn new(
        device: &Device,
        compiled_shader: Cow<'_, str>,
        target_layer_format: TextureFormat,
        external_var_layouts: &[BindGroupLayoutEntry],
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
                        format: target_layer_format,
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
                        format: target_layer_format,
                        view_dimension: TextureViewDimension::D2Array,
                    },
                    count: None,
                },
                BindGroupLayoutEntry {
                    binding: 8,
                    visibility: ShaderStages::COMPUTE,
                    ty: BindingType::StorageTexture {
                        access: StorageTextureAccess::ReadWrite,
                        format: target_layer_format,
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
            entries.extend_from_slice(external_var_layouts);
            entries
        };

        let layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some("brush main layout"),
            entries: &layout_entries,
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
            bind_group: None,
            layout,
            pipeline,
        }
    }

    pub fn prepare(
        &mut self,
        device: &Device,
        output_samples: &DynamicBuffer<OutputSamples>,
        stroke_info: &DynamicBuffer<StrokeInfo>,
        referenced_textures: &TextureAtlas,
        target_layer_tile_info: &Buffer,
        target_layer: &TextureView,
        intermediate_buffers: &DynamicIntermediateBuffer,
        dab_infos: &DynamicBuffer<DabInfos>,
        pass_fence: &DynamicBuffer<PassFence>,
        external_var_buffers: &[Buffer],
    ) {
        let mut entries = vec![
            BindGroupEntry {
                binding: 0,
                resource: output_samples.binding().unwrap(),
            },
            BindGroupEntry {
                binding: 1,
                resource: stroke_info.binding().unwrap(),
            },
            BindGroupEntry {
                binding: 2,
                resource: BindingResource::TextureView(referenced_textures.texture_view()),
            },
            BindGroupEntry {
                binding: 3,
                resource: referenced_textures.atlas_bounds_buffer_binding(),
            },
            BindGroupEntry {
                binding: 4,
                resource: target_layer_tile_info.as_entire_binding(),
            },
            BindGroupEntry {
                binding: 5,
                resource: BindingResource::TextureView(target_layer),
            },
            BindGroupEntry {
                binding: 6,
                resource: intermediate_buffers.tile_info_buffer().as_entire_binding(),
            },
            BindGroupEntry {
                binding: 7,
                resource: BindingResource::TextureView(&intermediate_buffers.textures()[0]),
            },
            BindGroupEntry {
                binding: 8,
                resource: BindingResource::TextureView(&intermediate_buffers.textures()[1]),
            },
            BindGroupEntry {
                binding: 9,
                resource: dab_infos.binding().unwrap(),
            },
            BindGroupEntry {
                binding: 10,
                resource: pass_fence.binding().unwrap(),
            },
        ];
        entries.extend(external_var_entries(external_var_buffers));

        self.bind_group = Some(device.create_bind_group(&BindGroupDescriptor {
            label: Some("brush main bind group"),
            layout: &self.layout,
            entries: &entries,
        }));
    }

    pub fn dispatch(&self, ec: &mut CommandEncoder, main_dispatch: &Buffer) {
        let bind_group = self
            .bind_group
            .as_ref()
            .expect("BrushMainPipeline::prepare() must be called before dispatch()");

        ec.push_debug_group("brush preset main");
        {
            let mut pass = ec.begin_compute_pass(&ComputePassDescriptor {
                label: Some("brush main compute pass"),
                ..Default::default()
            });

            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, bind_group, &[]);
            pass.dispatch_workgroups_indirect(main_dispatch, 0);
        }
        ec.pop_debug_group();
    }
}
