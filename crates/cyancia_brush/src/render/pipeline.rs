use std::{
    borrow::Cow,
    num::{NonZeroU32, NonZeroU64},
};

use bevy_math::URect;
use bytemuck::Contiguous;
use cyancia_image::tile::{DynamicLayerStorage, GpuTileInfo, GpuTileStorageInner};
use cyancia_render::{buffer::DynamicBuffer, texture_atlas::TextureAtlas};
use encase::ShaderType;
use glam::{UVec3, UVec4};
use toml::de;
use wesl::{VirtualResolver, Wesl};
use wgpu::{
    BindGroup, BindGroupDescriptor, BindGroupEntry, BindGroupLayout, BindGroupLayoutDescriptor,
    BindGroupLayoutEntry, BindingResource, BindingType, Buffer, BufferAddress, BufferBindingType,
    BufferUsages, CommandEncoder, ComputePass, ComputePassDescriptor, ComputePipeline,
    ComputePipelineDescriptor, Device, PipelineLayoutDescriptor, Queue, ShaderModuleDescriptor,
    ShaderSource, ShaderStages, StorageTextureAccess, TextureFormat, TextureSampleType,
    TextureView, TextureViewDimension,
};

use crate::render::{
    ComputedPenInput, DabInfo, EXTERNAL_VARIABLE_BASE_BINDING, StrokePostprocessData,
    StrokeResources,
};

pub struct BrushMainPipeline {
    layout: BindGroupLayout,
    pipeline: ComputePipeline,
}

impl BrushMainPipeline {
    pub fn new(
        device: &Device,
        resources: &StrokeResources,
        compiled_shader: Cow<'_, str>,
    ) -> Self {
        let layout_entries = bind_group_layout_entries(
            &resources.external_var_layouts,
            false,
            resources.target_layer_format.wgpu_format(),
        );

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

        Self { pipeline, layout }
    }

    pub fn dispatch(
        &self,
        device: &Device,
        pass: &mut ComputePass,
        target_layer_texture: &TextureView,
        target_layer_tile_info: &Buffer,
        samples: &DynamicBuffer<ComputedPenInput>,
        samples_offsets: &[u32],
        dab_infos: &DynamicBuffer<DabInfo>,
        dab_info_offsets: &[u32],
        resources: &StrokeResources,
        intermediate_buffers: &[DynamicLayerStorage; 2],
        round: &mut u32,
    ) {
        let bind_group_entries_even = bind_group_entries(
            resources,
            target_layer_texture,
            target_layer_tile_info,
            intermediate_buffers,
            Some(samples),
            None,
            dab_infos,
            true,
        );
        let bind_group_even = device.create_bind_group(&BindGroupDescriptor {
            label: Some("brush main bind group even"),
            layout: &self.layout,
            entries: &bind_group_entries_even,
        });

        let bind_group_entries_odd = bind_group_entries(
            resources,
            target_layer_texture,
            target_layer_tile_info,
            intermediate_buffers,
            Some(samples),
            None,
            dab_infos,
            false,
        );
        let bind_group_odd = device.create_bind_group(&BindGroupDescriptor {
            label: Some("brush main bind group odd"),
            layout: &self.layout,
            entries: &bind_group_entries_odd,
        });

        let n_tiles = intermediate_buffers[0].len();

        pass.push_debug_group("brush preset main");
        {
            pass.set_pipeline(&self.pipeline);

            for i in 0..samples_offsets.len() {
                pass.push_debug_group(&format!("brush preset main dispatch {}", i));
                pass.set_bind_group(
                    0,
                    if *round % 2 == 0 {
                        &bind_group_even
                    } else {
                        &bind_group_odd
                    },
                    &[samples_offsets[i], dab_info_offsets[i]],
                );
                pass.dispatch_workgroups(
                    GpuTileStorageInner::TILE_SIZE.div_ceil(16),
                    GpuTileStorageInner::TILE_SIZE.div_ceil(16),
                    n_tiles as u32,
                );
                pass.pop_debug_group();
                *round += 1;
            }
        }
        pass.pop_debug_group();

        log::info!(
            "Dispatched {} main passes, next round {}.",
            samples_offsets.len(),
            round,
        );
    }
}

pub struct BrushPostProcessPipeline {
    layout: BindGroupLayout,
    pipeline: ComputePipeline,
}

impl BrushPostProcessPipeline {
    pub fn new(
        device: &Device,
        resources: &StrokeResources,
        compiled_shader: Cow<'_, str>,
    ) -> Self {
        let layout_entries = bind_group_layout_entries(
            &resources.external_var_layouts,
            true,
            resources.target_layer_format.wgpu_format(),
        );

        let layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some("brush postprocess layout"),
            entries: &layout_entries,
        });

        let shader = device.create_shader_module(ShaderModuleDescriptor {
            label: Some("brush postprocess shader"),
            source: ShaderSource::Wgsl(compiled_shader),
        });

        let pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: Some("brush postprocess pipeline layout"),
            bind_group_layouts: &[&layout],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_compute_pipeline(&ComputePipelineDescriptor {
            label: Some("brush postprocess pipeline"),
            layout: Some(&pipeline_layout),
            module: &shader,
            entry_point: Some("main"),
            compilation_options: Default::default(),
            cache: None,
        });

        Self { pipeline, layout }
    }

    pub fn dispatch(
        &self,
        device: &Device,
        pass: &mut ComputePass,
        stroke_pp_data: &DynamicBuffer<StrokePostprocessData>,
        target_layer_texture: &TextureView,
        target_layer_tile_info: &Buffer,
        dab_infos: &DynamicBuffer<DabInfo>,
        resources: &StrokeResources,
        intermediate_buffers: &[DynamicLayerStorage; 2],
        round: &mut u32,
    ) {
        let bind_group_entries = bind_group_entries(
            resources,
            target_layer_texture,
            target_layer_tile_info,
            intermediate_buffers,
            None,
            Some(stroke_pp_data),
            dab_infos,
            *round % 2 == 0,
        );
        let bind_group = device.create_bind_group(&BindGroupDescriptor {
            label: Some("brush postprocess bind group"),
            layout: &self.layout,
            entries: &bind_group_entries,
        });

        pass.push_debug_group("brush preset postprocess");
        {
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &bind_group, &[]);
            pass.dispatch_workgroups(
                GpuTileStorageInner::TILE_SIZE.div_ceil(16),
                GpuTileStorageInner::TILE_SIZE.div_ceil(16),
                intermediate_buffers[0].len() as u32,
            );
        }
        pass.pop_debug_group();
        *round += 1;
    }
}

fn bind_group_layout_entries(
    external_var: &[BindGroupLayoutEntry],
    is_postprocess: bool,
    target_layer_format: TextureFormat,
) -> Vec<BindGroupLayoutEntry> {
    let mut entries = vec![
        BindGroupLayoutEntry {
            binding: 0,
            visibility: ShaderStages::COMPUTE,
            ty: BindingType::Buffer {
                ty: BufferBindingType::Storage { read_only: true },
                has_dynamic_offset: !is_postprocess,
                min_binding_size: Some(if is_postprocess {
                    StrokePostprocessData::min_size()
                } else {
                    ComputedPenInput::min_size()
                }),
            },
            count: None,
        },
        BindGroupLayoutEntry {
            binding: 1,
            visibility: ShaderStages::COMPUTE,
            ty: BindingType::Texture {
                sample_type: TextureSampleType::Float { filterable: false },
                view_dimension: TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        },
        BindGroupLayoutEntry {
            binding: 2,
            visibility: ShaderStages::COMPUTE,
            ty: BindingType::Buffer {
                ty: BufferBindingType::Storage { read_only: true },
                has_dynamic_offset: false,
                min_binding_size: Some(URect::min_size()),
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
            ty: BindingType::StorageTexture {
                access: StorageTextureAccess::ReadOnly,
                format: target_layer_format,
                view_dimension: TextureViewDimension::D2Array,
            },
            count: None,
        },
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
        BindGroupLayoutEntry {
            binding: 6,
            visibility: ShaderStages::COMPUTE,
            ty: BindingType::StorageTexture {
                access: StorageTextureAccess::ReadOnly,
                format: target_layer_format,
                view_dimension: TextureViewDimension::D2Array,
            },
            count: None,
        },
        BindGroupLayoutEntry {
            binding: 7,
            visibility: ShaderStages::COMPUTE,
            ty: BindingType::StorageTexture {
                access: StorageTextureAccess::WriteOnly,
                format: target_layer_format,
                view_dimension: TextureViewDimension::D2Array,
            },
            count: None,
        },
        BindGroupLayoutEntry {
            binding: 8,
            visibility: ShaderStages::COMPUTE,
            ty: BindingType::Buffer {
                ty: BufferBindingType::Storage { read_only: false },
                has_dynamic_offset: !is_postprocess,
                min_binding_size: Some(DabInfo::min_size()),
            },
            count: None,
        },
    ];
    entries.extend(external_var);
    if !is_postprocess {}
    entries
}

fn bind_group_entries<'a>(
    resources: &'a StrokeResources,
    target_layer_texture: &'a TextureView,
    target_layer_tile_info: &'a Buffer,
    intermediate_buffers: &'a [DynamicLayerStorage],
    samples: Option<&'a DynamicBuffer<ComputedPenInput>>,
    stroke_pp_data: Option<&'a DynamicBuffer<StrokePostprocessData>>,
    dab_infos: &'a DynamicBuffer<DabInfo>,
    is_even: bool,
) -> Vec<BindGroupEntry<'a>> {
    let mut entries = vec![
        BindGroupEntry {
            binding: 1,
            resource: BindingResource::TextureView(resources.referenced_textures.texture_view()),
        },
        BindGroupEntry {
            binding: 2,
            resource: resources.referenced_textures.atlas_bounds_buffer_binding(),
        },
        BindGroupEntry {
            binding: 3,
            resource: target_layer_tile_info.as_entire_binding(),
        },
        BindGroupEntry {
            binding: 4,
            resource: BindingResource::TextureView(target_layer_texture),
        },
        BindGroupEntry {
            binding: 5,
            resource: intermediate_buffers[0]
                .tile_info_buffer()
                .unwrap()
                .as_entire_binding(),
        },
        BindGroupEntry {
            binding: 8,
            resource: dab_infos.binding().unwrap(),
        },
    ];
    entries.extend(resources.external_var_bindings());
    if let Some(samples) = samples {
        entries.push(BindGroupEntry {
            binding: 0,
            resource: samples.binding().unwrap(),
        });
    } else if let Some(stroke_pp_data) = stroke_pp_data {
        entries.push(BindGroupEntry {
            binding: 0,
            resource: stroke_pp_data.binding().unwrap(),
        });
    }

    if is_even {
        entries.extend([
            BindGroupEntry {
                binding: 6,
                resource: BindingResource::TextureView(&intermediate_buffers[0].texture().unwrap()),
            },
            BindGroupEntry {
                binding: 7,
                resource: BindingResource::TextureView(&intermediate_buffers[1].texture().unwrap()),
            },
        ]);
    } else {
        entries.extend([
            BindGroupEntry {
                binding: 6,
                resource: BindingResource::TextureView(&intermediate_buffers[1].texture().unwrap()),
            },
            BindGroupEntry {
                binding: 7,
                resource: BindingResource::TextureView(&intermediate_buffers[0].texture().unwrap()),
            },
        ]);
    }

    entries
}
