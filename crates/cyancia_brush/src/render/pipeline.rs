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
    ShaderSource, ShaderStages, StorageTextureAccess, TextureSampleType, TextureView,
    TextureViewDimension,
};

use crate::render::{
    ComputedPenInput, DabInfo, DabInfos, EXTERNAL_VARIABLE_BASE_BINDING, OutputSamples, PenInput,
    PenInputSampler, StrokeInfo, StrokeResources,
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
        let layout_entries = {
            let mut entries = vec![
                BindGroupLayoutEntry {
                    binding: 0,
                    visibility: ShaderStages::COMPUTE,
                    ty: BindingType::Buffer {
                        ty: BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: true,
                        min_binding_size: Some(ComputedPenInput::min_size()),
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
                        format: resources.target_layer.texture().format(),
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
                        format: resources.target_layer_info.texel_type.wgpu_format(),
                        view_dimension: TextureViewDimension::D2Array,
                    },
                    count: None,
                },
                BindGroupLayoutEntry {
                    binding: 7,
                    visibility: ShaderStages::COMPUTE,
                    ty: BindingType::StorageTexture {
                        access: StorageTextureAccess::WriteOnly,
                        format: resources.target_layer_info.texel_type.wgpu_format(),
                        view_dimension: TextureViewDimension::D2Array,
                    },
                    count: None,
                },
                BindGroupLayoutEntry {
                    binding: 8,
                    visibility: ShaderStages::COMPUTE,
                    ty: BindingType::Buffer {
                        ty: BufferBindingType::Storage { read_only: false },
                        has_dynamic_offset: true,
                        min_binding_size: Some(DabInfo::min_size()),
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
        samples: &DynamicBuffer<ComputedPenInput>,
        samples_offsets: &[u32],
        dab_infos: &DynamicBuffer<DabInfo>,
        dab_info_offsets: &[u32],
        resources: &StrokeResources,
        intermediate_buffers: &[DynamicLayerStorage; 2],
        params: &[UVec3],
        round: &mut u32,
    ) {
        let bind_group_entries = {
            let mut entries = vec![
                BindGroupEntry {
                    binding: 0,
                    resource: samples.binding().unwrap(),
                },
                BindGroupEntry {
                    binding: 1,
                    resource: BindingResource::TextureView(
                        resources.referenced_textures.texture_view(),
                    ),
                },
                BindGroupEntry {
                    binding: 2,
                    resource: resources.referenced_textures.atlas_bounds_buffer_binding(),
                },
                BindGroupEntry {
                    binding: 3,
                    resource: resources.target_layer_tile_info.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 4,
                    resource: BindingResource::TextureView(&resources.target_layer),
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
            entries
        };

        let intermediate_textures = [
            intermediate_buffers[0].texture().unwrap(),
            intermediate_buffers[1].texture().unwrap(),
        ];
        let bind_group_entries_even = {
            let mut entries = bind_group_entries.clone();
            entries.extend([
                BindGroupEntry {
                    binding: 6,
                    resource: BindingResource::TextureView(&intermediate_textures[0]),
                },
                BindGroupEntry {
                    binding: 7,
                    resource: BindingResource::TextureView(&intermediate_textures[1]),
                },
            ]);
            entries
        };
        let bind_group_even = device.create_bind_group(&BindGroupDescriptor {
            label: Some("brush main bind group even"),
            layout: &self.layout,
            entries: &bind_group_entries_even,
        });

        let bind_group_entries_odd = {
            let mut entries = bind_group_entries.clone();
            entries.extend([
                BindGroupEntry {
                    binding: 6,
                    resource: BindingResource::TextureView(&intermediate_textures[1]),
                },
                BindGroupEntry {
                    binding: 7,
                    resource: BindingResource::TextureView(&intermediate_textures[0]),
                },
            ]);
            entries
        };
        let bind_group_odd = device.create_bind_group(&BindGroupDescriptor {
            label: Some("brush main bind group odd"),
            layout: &self.layout,
            entries: &bind_group_entries_odd,
        });

        pass.push_debug_group("brush preset main");
        {
            pass.set_pipeline(&self.pipeline);

            for (i, param) in params.iter().enumerate() {
                pass.push_debug_group(&format!("brush preset main dispatch {}", i));
                pass.set_bind_group(
                    0,
                    // TODO: Accumulate previous dabs.
                    if *round % 2 == 0 {
                        &bind_group_even
                    } else {
                        &bind_group_odd
                    },
                    &[samples_offsets[i], dab_info_offsets[i]],
                );
                pass.dispatch_workgroups(param.x, param.y, param.z);
                pass.pop_debug_group();
                *round += 1;
            }
        }
        pass.pop_debug_group();

        log::info!(
            "Dispatched {} main passes, next round {}.",
            params.len(),
            round,
        );
    }
}
