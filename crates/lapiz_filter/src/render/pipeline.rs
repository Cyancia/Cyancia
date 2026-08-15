use std::borrow::Cow;

use bevy_math::URect;
use lapiz_image::tile::{DynamicLayerStorage, GpuTileInfo, GpuTileStorage, LayerBinding};
use lapiz_render::{
    bind_group_entries::DynamicBindGroupEntries,
    bind_group_layout_entries::{DynamicBindGroupLayoutEntries, binding_types},
};
use wgpu::{
    BindGroupDescriptor, BindGroupEntry, BindGroupLayout, BindGroupLayoutDescriptor,
    BindGroupLayoutEntry, BindingResource, Buffer, ComputePass, ComputePipeline,
    ComputePipelineDescriptor, Device, PipelineLayoutDescriptor, ShaderModuleDescriptor,
    ShaderSource, ShaderStages, StorageTextureAccess, TextureSampleType,
};

use crate::render::FilterResources;

fn tile_info_storage_layout_entry(binding: u32) -> BindGroupLayoutEntry {
    binding_types::storage_buffer_read_only::<GpuTileInfo>(false)
        .build(binding, ShaderStages::COMPUTE)
}

fn output_texture_layout_entry(binding: u32, format: wgpu::TextureFormat) -> BindGroupLayoutEntry {
    binding_types::texture_storage_2d_array(format, StorageTextureAccess::WriteOnly)
        .build(binding, ShaderStages::COMPUTE)
}

/// Bindings shared by main and bounds-eval:
/// 0 input texture, 1 input tile info, 4 selection texture,
/// 5 selection tile info, 6 has_selection flag, 7 input bounds rect.
fn filter_common_layout_entries(resources: &FilterResources) -> Vec<BindGroupLayoutEntry> {
    DynamicBindGroupLayoutEntries::new_with_indices(
        ShaderStages::COMPUTE,
        (
            (
                0,
                binding_types::texture_storage_2d_array(
                    resources.target_layer_format.wgpu_format(),
                    StorageTextureAccess::ReadOnly,
                ),
            ),
            (
                1,
                binding_types::storage_buffer_read_only::<GpuTileInfo>(false),
            ),
            (
                4,
                binding_types::texture_storage_2d_array(
                    resources.selection_layer_format.wgpu_format(),
                    StorageTextureAccess::ReadOnly,
                ),
            ),
            (
                5,
                binding_types::storage_buffer_read_only::<GpuTileInfo>(false),
            ),
            (6, binding_types::storage_buffer_read_only::<u32>(false)),
            (7, binding_types::storage_buffer_read_only::<URect>(false)),
            (
                11,
                binding_types::texture_2d(TextureSampleType::Float { filterable: false }),
            ),
            (12, binding_types::storage_buffer_read_only::<URect>(false)),
        ),
    )
    .to_vec()
}

#[allow(clippy::too_many_arguments)]
fn filter_common_entries<'a>(
    resources: &'a FilterResources,
    input: &'a LayerBinding,
    selection: &'a LayerBinding,
    has_selection: &'a Buffer,
    bounds_input: &'a Buffer,
) -> Vec<BindGroupEntry<'a>> {
    let mut entries = DynamicBindGroupEntries::new_with_indices((
        (0, BindingResource::TextureView(&input.texture)),
        (1, input.tile_info_buffer.as_entire_binding()),
        (4, BindingResource::TextureView(&selection.texture)),
        (5, selection.tile_info_buffer.as_entire_binding()),
        (6, has_selection.as_entire_binding()),
        (7, bounds_input.as_entire_binding()),
        (
            11,
            BindingResource::TextureView(resources.texture_atlas.texture_view()),
        ),
        (12, resources.texture_atlas.atlas_bounds_buffer_binding()),
    ))
    .to_vec();
    entries.extend(resources.external_var_bindings());
    entries
}

/// Computed main pipeline. Reads an input buffer at bindings 0/1, the original
/// layer at bindings 9/10 and writes per-out-tile pixels at bindings 2/3.
#[derive(Clone)]
pub struct FilterMainPipeline {
    layout: BindGroupLayout,
    pipeline: ComputePipeline,
}

impl FilterMainPipeline {
    pub fn new(
        device: &Device,
        resources: &FilterResources,
        compiled_shader: Cow<'_, str>,
    ) -> Self {
        let mut layout_entries = filter_common_layout_entries(resources);
        layout_entries.push(output_texture_layout_entry(
            2,
            resources.target_layer_format.wgpu_format(),
        ));
        layout_entries.push(tile_info_storage_layout_entry(3));
        layout_entries.push(
            binding_types::texture_storage_2d_array(
                resources.target_layer_format.wgpu_format(),
                StorageTextureAccess::ReadOnly,
            )
            .build(9, ShaderStages::COMPUTE),
        );
        layout_entries.push(tile_info_storage_layout_entry(10));
        layout_entries.extend(resources.external_var_layouts.clone());
        let layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some("filter main layout"),
            entries: &layout_entries,
        });

        let shader = device.create_shader_module(ShaderModuleDescriptor {
            label: Some("filter main shader"),
            source: ShaderSource::Wgsl(compiled_shader),
        });
        let pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: Some("filter main pipeline layout"),
            bind_group_layouts: &[&layout],
            push_constant_ranges: &[],
        });
        let pipeline = device.create_compute_pipeline(&ComputePipelineDescriptor {
            label: Some("filter main pipeline"),
            layout: Some(&pipeline_layout),
            module: &shader,
            entry_point: Some("filter_main"),
            compilation_options: Default::default(),
            cache: None,
        });

        Self { layout, pipeline }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn dispatch(
        &self,
        device: &Device,
        pass: &mut ComputePass,
        input: &LayerBinding,
        output: &DynamicLayerStorage,
        selection: &LayerBinding,
        has_selection: &Buffer,
        bounds_input: &Buffer,
        original: &LayerBinding,
        resources: &FilterResources,
    ) {
        let mut entries =
            filter_common_entries(resources, input, selection, has_selection, bounds_input);
        entries.extend(
            DynamicBindGroupEntries::new_with_indices((
                (
                    2,
                    BindingResource::TextureView(output.texture_view().unwrap()),
                ),
                (3, output.tile_info_buffer().unwrap().as_entire_binding()),
                (9, BindingResource::TextureView(&original.texture)),
                (10, original.tile_info_buffer.as_entire_binding()),
            ))
            .to_vec(),
        );

        let bind_group = device.create_bind_group(&BindGroupDescriptor {
            label: Some("filter main bind group"),
            layout: &self.layout,
            entries: &entries,
        });

        let n_tiles = output
            .texture_view()
            .unwrap()
            .texture()
            .depth_or_array_layers();

        pass.push_debug_group("filter main");
        {
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &bind_group, &[]);
            pass.dispatch_workgroups(
                GpuTileStorage::TILE_SIZE.div_ceil(16),
                GpuTileStorage::TILE_SIZE.div_ceil(16),
                n_tiles,
            );
        }
        pass.pop_debug_group();
    }
}

/// Computes the output pixel rectangle of a group given its input rectangle.
/// Reads the input rect at binding 7, runs the group graph, writes the output
/// bounds into the buffer at binding 8, read back asynchronously by the caller.
#[derive(Clone)]
pub struct FilterBoundsEvalPipeline {
    layout: BindGroupLayout,
    pipeline: ComputePipeline,
}

impl FilterBoundsEvalPipeline {
    pub fn new(
        device: &Device,
        resources: &FilterResources,
        compiled_shader: Cow<'_, str>,
    ) -> Self {
        let mut layout_entries = filter_common_layout_entries(resources);
        layout_entries
            .push(binding_types::storage_buffer::<URect>(false).build(8, ShaderStages::COMPUTE));
        layout_entries.extend(resources.external_var_layouts.clone());

        let layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some("filter bounds eval layout"),
            entries: &layout_entries,
        });
        let shader = device.create_shader_module(ShaderModuleDescriptor {
            label: Some("filter bounds eval shader"),
            source: ShaderSource::Wgsl(compiled_shader),
        });
        let pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: Some("filter bounds eval pipeline layout"),
            bind_group_layouts: &[&layout],
            push_constant_ranges: &[],
        });
        let pipeline = device.create_compute_pipeline(&ComputePipelineDescriptor {
            label: Some("filter bounds eval pipeline"),
            layout: Some(&pipeline_layout),
            module: &shader,
            entry_point: Some("filter_bounds_eval"),
            compilation_options: Default::default(),
            cache: None,
        });

        Self { pipeline, layout }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn dispatch(
        &self,
        device: &Device,
        pass: &mut ComputePass,
        input: &LayerBinding,
        selection: &LayerBinding,
        has_selection: &Buffer,
        bounds_input: &Buffer,
        bounds_output: &Buffer,
        resources: &FilterResources,
    ) {
        let mut entries =
            filter_common_entries(resources, input, selection, has_selection, bounds_input);
        entries.extend(
            DynamicBindGroupEntries::new_with_indices(((8, bounds_output.as_entire_binding()),))
                .to_vec(),
        );

        let bind_group = device.create_bind_group(&BindGroupDescriptor {
            label: Some("filter bounds eval bind group"),
            layout: &self.layout,
            entries: &entries,
        });

        pass.push_debug_group("filter bounds eval");
        {
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &bind_group, &[]);
            pass.dispatch_workgroups(1, 1, 1);
        }
        pass.pop_debug_group();
    }
}
