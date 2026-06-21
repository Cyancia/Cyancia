use std::borrow::Cow;

use bevy_math::URect;
use cyancia_image::tile::{DynamicLayerStorage, GpuTileInfo, GpuTileStorage};
use cyancia_render::{
    bind_group_entries::DynamicBindGroupEntries,
    bind_group_layout_entries::{DynamicBindGroupLayoutEntries, binding_types},
    buffer::DynamicBuffer,
};
use wgpu::{
    BindGroupDescriptor, BindGroupEntry, BindGroupLayout, BindGroupLayoutDescriptor,
    BindGroupLayoutEntry, BindingResource, Buffer, ComputePass, ComputePipeline,
    ComputePipelineDescriptor, Device, PipelineLayoutDescriptor, ShaderModuleDescriptor,
    ShaderSource, ShaderStages, StorageTextureAccess, TextureFormat, TextureSampleType,
    TextureView,
};

use crate::render::{ComputedPenInput, DabInfo, StrokePostprocessData, StrokeResources};

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
            resources.selection_layer_format.wgpu_format(),
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
            bind_group_layouts: &[Some(&layout)],
            ..Default::default()
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
        has_selection: &Buffer,
        selection_layer_texture: &TextureView,
        selection_layer_tile_info: &Buffer,
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
            has_selection,
            selection_layer_texture,
            selection_layer_tile_info,
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
            has_selection,
            selection_layer_texture,
            selection_layer_tile_info,
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
                    if (*round).is_multiple_of(2) {
                        &bind_group_even
                    } else {
                        &bind_group_odd
                    },
                    &[samples_offsets[i], dab_info_offsets[i]],
                );
                pass.dispatch_workgroups(
                    GpuTileStorage::TILE_SIZE.div_ceil(16),
                    GpuTileStorage::TILE_SIZE.div_ceil(16),
                    n_tiles as u32,
                );
                pass.pop_debug_group();
                *round += 1;
            }
        }
        pass.pop_debug_group();

        log::debug!(
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
            resources.selection_layer_format.wgpu_format(),
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
            bind_group_layouts: &[Some(&layout)],
            ..Default::default()
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
        has_selection: &Buffer,
        selection_layer_texture: &TextureView,
        selection_layer_tile_info: &Buffer,
        dab_infos: &DynamicBuffer<DabInfo>,
        resources: &StrokeResources,
        intermediate_buffers: &[DynamicLayerStorage; 2],
        round: &mut u32,
    ) {
        let bind_group_entries = bind_group_entries(
            resources,
            target_layer_texture,
            target_layer_tile_info,
            has_selection,
            selection_layer_texture,
            selection_layer_tile_info,
            intermediate_buffers,
            None,
            Some(stroke_pp_data),
            dab_infos,
            (*round).is_multiple_of(2),
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
                GpuTileStorage::TILE_SIZE.div_ceil(16),
                GpuTileStorage::TILE_SIZE.div_ceil(16),
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
    selection_layer_format: TextureFormat,
) -> Vec<BindGroupLayoutEntry> {
    let mut entries = DynamicBindGroupLayoutEntries::sequential(
        ShaderStages::COMPUTE,
        (
            if is_postprocess {
                binding_types::storage_buffer_read_only::<StrokePostprocessData>(false)
            } else {
                binding_types::storage_buffer_read_only::<ComputedPenInput>(true)
            },
            binding_types::texture_2d(TextureSampleType::Float { filterable: false }),
            binding_types::storage_buffer_read_only::<URect>(false),
            binding_types::storage_buffer_read_only::<GpuTileInfo>(false),
            binding_types::texture_storage_2d_array(
                target_layer_format,
                StorageTextureAccess::ReadOnly,
            ),
            binding_types::storage_buffer_read_only::<GpuTileInfo>(false),
            binding_types::texture_storage_2d_array(
                target_layer_format,
                StorageTextureAccess::ReadOnly,
            ),
            binding_types::texture_storage_2d_array(
                target_layer_format,
                StorageTextureAccess::WriteOnly,
            ),
            if is_postprocess {
                binding_types::storage_buffer::<DabInfo>(false)
            } else {
                binding_types::storage_buffer::<DabInfo>(true)
            },
            binding_types::texture_storage_2d_array(
                selection_layer_format,
                StorageTextureAccess::ReadOnly,
            ),
            binding_types::storage_buffer_read_only::<GpuTileInfo>(false),
            binding_types::storage_buffer_read_only::<u32>(false),
        ),
    )
    .to_vec();
    entries.extend_from_slice(external_var);
    entries
}

fn bind_group_entries<'a>(
    resources: &'a StrokeResources,
    target_layer_texture: &'a TextureView,
    target_layer_tile_info: &'a Buffer,
    has_selection: &'a Buffer,
    selection_layer_texture: &'a TextureView,
    selection_layer_tile_info: &'a Buffer,
    intermediate_buffers: &'a [DynamicLayerStorage],
    samples: Option<&'a DynamicBuffer<ComputedPenInput>>,
    stroke_pp_data: Option<&'a DynamicBuffer<StrokePostprocessData>>,
    dab_infos: &'a DynamicBuffer<DabInfo>,
    is_even: bool,
) -> Vec<BindGroupEntry<'a>> {
    let mut entries = DynamicBindGroupEntries::new_with_indices((
        (
            1,
            BindingResource::TextureView(resources.referenced_textures.texture_view()),
        ),
        (
            2,
            resources.referenced_textures.atlas_bounds_buffer_binding(),
        ),
        (3, target_layer_tile_info.as_entire_binding()),
        (4, BindingResource::TextureView(target_layer_texture)),
        (
            5,
            intermediate_buffers[0]
                .tile_info_buffer()
                .unwrap()
                .as_entire_binding(),
        ),
        (8, dab_infos.binding().unwrap()),
        (9, BindingResource::TextureView(selection_layer_texture)),
        (10, selection_layer_tile_info.as_entire_binding()),
        (11, has_selection.as_entire_binding()),
    ));
    entries.entries.extend(resources.external_var_bindings());

    if let Some(samples) = samples {
        entries.entries.push(BindGroupEntry {
            binding: 0,
            resource: samples.binding().unwrap(),
        });
    } else if let Some(stroke_pp_data) = stroke_pp_data {
        entries.entries.push(BindGroupEntry {
            binding: 0,
            resource: stroke_pp_data.binding().unwrap(),
        });
    }

    let (read_idx, write_idx) = if is_even { (0, 1) } else { (1, 0) };
    entries.entries.push(BindGroupEntry {
        binding: 6,
        resource: BindingResource::TextureView(
            intermediate_buffers[read_idx].texture_view().unwrap(),
        ),
    });
    entries.entries.push(BindGroupEntry {
        binding: 7,
        resource: BindingResource::TextureView(
            intermediate_buffers[write_idx].texture_view().unwrap(),
        ),
    });

    entries.entries
}
