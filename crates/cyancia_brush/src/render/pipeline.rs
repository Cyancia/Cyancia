use std::borrow::Cow;

use bevy_math::URect;
use cyancia_image::tile::{DynamicLayerStorage, GpuTileInfo, GpuTileStorage};
use cyancia_render::{
    bind_group_entries::{BindGroupEntries, DynamicBindGroupEntries},
    bind_group_layout_entries::{
        BindGroupLayoutEntries, DynamicBindGroupLayoutEntries, binding_types,
    },
    buffer::{BufferVec, DynamicBuffer},
};
use glam::UVec4;
use wgpu::{
    BindGroupDescriptor, BindGroupEntry, BindGroupLayout, BindGroupLayoutDescriptor,
    BindGroupLayoutEntry, BindingResource, Buffer, ComputePass, ComputePipeline,
    ComputePipelineDescriptor, Device, PipelineLayoutDescriptor, ShaderModuleDescriptor,
    ShaderSource, ShaderStages, StorageTextureAccess, TextureFormat, TextureSampleType,
    TextureView,
};

use crate::{
    input_processing::RawPenInput,
    render::{
        ComputedPenInput, DabInfo, InputSampler, OutputSamples, PenInput, StrokePostprocessData,
        StrokeResources,
    },
};

pub struct BrushInputSamplingPipeline {
    layout: BindGroupLayout,
    pipeline: ComputePipeline,
}

impl BrushInputSamplingPipeline {
    pub fn new(device: &Device, compiled_shader: Cow<'_, str>) -> Self {
        let layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some("brush input sampling layout"),
            entries: BindGroupLayoutEntries::sequential(
                ShaderStages::COMPUTE,
                (
                    binding_types::storage_buffer_read_only::<PenInput>(false),
                    binding_types::storage_buffer::<InputSampler>(false),
                    binding_types::storage_buffer::<OutputSamples>(false),
                    binding_types::storage_buffer::<UVec4>(false),
                ),
            )
            .as_ref(),
        });

        let shader = device.create_shader_module(ShaderModuleDescriptor {
            label: Some("brush input sampling shader"),
            source: ShaderSource::Wgsl(compiled_shader),
        });

        let pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: Some("brush input sampling pipeline layout"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });

        let pipeline = device.create_compute_pipeline(&ComputePipelineDescriptor {
            label: Some("brush input sampling pipeline"),
            layout: Some(&pipeline_layout),
            module: &shader,
            entry_point: Some("main"),
            compilation_options: Default::default(),
            cache: None,
        });

        Self { layout, pipeline }
    }

    pub fn dispatch(
        &self,
        device: &Device,
        pass: &mut ComputePass,
        pen_input: &DynamicBuffer<PenInput>,
        input_sampler: &DynamicBuffer<InputSampler>,
        output_samples: &BufferVec<ComputedPenInput>,
        bounds_eval_dispatch: &Buffer,
    ) {
        let bind_group = device.create_bind_group(&BindGroupDescriptor {
            label: Some("brush input sampling bind group"),
            layout: &self.layout,
            entries: BindGroupEntries::sequential((
                pen_input.binding().unwrap(),
                input_sampler.binding().unwrap(),
                output_samples.binding().unwrap(),
                bounds_eval_dispatch.as_entire_binding(),
            ))
            .as_ref(),
        });

        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &bind_group, &[]);
        pass.dispatch_workgroups(1, 1, 1);
    }
}

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
            None,
            None,
            Some(dab_infos),
            None,
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
            None,
            None,
            Some(dab_infos),
            None,
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
            false,
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
            None,
            Some(stroke_pp_data),
            None,
            Some(dab_infos),
            None,
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

pub struct BrushMainBoundsEvalPipeline {
    layout: BindGroupLayout,
    pipeline: ComputePipeline,
}

impl BrushMainBoundsEvalPipeline {
    pub fn new(
        device: &Device,
        resources: &StrokeResources,
        compiled_shader: Cow<'_, str>,
    ) -> Self {
        let layout_entries = bind_group_layout_entries(
            &resources.external_var_layouts,
            false,
            true,
            resources.target_layer_format.wgpu_format(),
            resources.selection_layer_format.wgpu_format(),
        );

        let layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some("brush main bounds eval layout"),
            entries: &layout_entries,
        });

        let shader = device.create_shader_module(ShaderModuleDescriptor {
            label: Some("brush main bounds eval shader"),
            source: ShaderSource::Wgsl(compiled_shader),
        });

        let pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: Some("brush main bounds eval pipeline layout"),
            bind_group_layouts: &[Some(&layout)],
            ..Default::default()
        });

        let pipeline = device.create_compute_pipeline(&ComputePipelineDescriptor {
            label: Some("brush main bounds eval pipeline"),
            layout: Some(&pipeline_layout),
            module: &shader,
            entry_point: Some("main_bounds_eval"),
            compilation_options: Default::default(),
            cache: None,
        });

        Self { pipeline, layout }
    }

    pub fn dispatch(
        &self,
        device: &Device,
        pass: &mut ComputePass,
        samples: &BufferVec<ComputedPenInput>,
        dab_infos: &BufferVec<DabInfo>,
        target_layer_texture: &TextureView,
        target_layer_tile_info: &Buffer,
        has_selection: &Buffer,
        selection_layer_texture: &TextureView,
        selection_layer_tile_info: &Buffer,
        resources: &StrokeResources,
        intermediate_buffers: &[DynamicLayerStorage; 2],
        round: &mut u32,
    ) {
        use crate::render::MAX_DABS_PER_STROKE;

        let bind_group_entries = bind_group_entries(
            resources,
            target_layer_texture,
            target_layer_tile_info,
            has_selection,
            selection_layer_texture,
            selection_layer_tile_info,
            intermediate_buffers,
            None,
            Some(samples),
            None,
            None,
            None,
            Some(dab_infos),
            (*round).is_multiple_of(2),
        );
        let bind_group = device.create_bind_group(&BindGroupDescriptor {
            label: Some("brush main bounds eval bind group"),
            layout: &self.layout,
            entries: &bind_group_entries,
        });

        pass.push_debug_group("brush preset main bounds eval");
        {
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &bind_group, &[]);
            pass.dispatch_workgroups(1, 1, MAX_DABS_PER_STROKE.div_ceil(16));
        }
        pass.pop_debug_group();
        *round += 1;
    }
}

pub struct BrushPostProcessBoundsEvalPipeline {
    layout: BindGroupLayout,
    pipeline: ComputePipeline,
}

impl BrushPostProcessBoundsEvalPipeline {
    pub fn new(
        device: &Device,
        resources: &StrokeResources,
        compiled_shader: Cow<'_, str>,
    ) -> Self {
        let layout_entries = bind_group_layout_entries(
            &resources.external_var_layouts,
            true,
            true,
            resources.target_layer_format.wgpu_format(),
            resources.selection_layer_format.wgpu_format(),
        );

        let layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some("brush postprocess bounds eval layout"),
            entries: &layout_entries,
        });

        let shader = device.create_shader_module(ShaderModuleDescriptor {
            label: Some("brush postprocess bounds eval shader"),
            source: ShaderSource::Wgsl(compiled_shader),
        });

        let pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: Some("brush postprocess bounds eval pipeline layout"),
            bind_group_layouts: &[Some(&layout)],
            ..Default::default()
        });

        let pipeline = device.create_compute_pipeline(&ComputePipelineDescriptor {
            label: Some("brush postprocess bounds eval pipeline"),
            layout: Some(&pipeline_layout),
            module: &shader,
            entry_point: Some("main_bounds_eval"),
            compilation_options: Default::default(),
            cache: None,
        });

        Self { pipeline, layout }
    }

    pub fn dispatch(
        &self,
        device: &Device,
        pass: &mut ComputePass,
        stroke_pp_data: &BufferVec<StrokePostprocessData>,
        target_layer_texture: &TextureView,
        target_layer_tile_info: &Buffer,
        has_selection: &Buffer,
        selection_layer_texture: &TextureView,
        selection_layer_tile_info: &Buffer,
        dab_infos: &BufferVec<DabInfo>,
        resources: &StrokeResources,
        intermediate_buffers: &[DynamicLayerStorage; 2],
        round: &mut u32,
    ) {
        use crate::render::MAX_DABS_PER_STROKE;

        let bind_group_entries = bind_group_entries(
            resources,
            target_layer_texture,
            target_layer_tile_info,
            has_selection,
            selection_layer_texture,
            selection_layer_tile_info,
            intermediate_buffers,
            None,
            None,
            None,
            Some(stroke_pp_data),
            None,
            Some(dab_infos),
            (*round).is_multiple_of(2),
        );
        let bind_group = device.create_bind_group(&BindGroupDescriptor {
            label: Some("brush postprocess bounds eval bind group"),
            layout: &self.layout,
            entries: &bind_group_entries,
        });

        pass.push_debug_group("brush preset postprocess bounds eval");
        {
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &bind_group, &[]);
            pass.dispatch_workgroups(1, 1, MAX_DABS_PER_STROKE.div_ceil(16));
        }
        pass.pop_debug_group();
        *round += 1;
    }
}

fn bind_group_layout_entries(
    external_var: &[BindGroupLayoutEntry],
    is_postprocess: bool,
    is_bounds_eval: bool,
    target_layer_format: TextureFormat,
    selection_layer_format: TextureFormat,
) -> Vec<BindGroupLayoutEntry> {
    let mut entries = DynamicBindGroupLayoutEntries::sequential(
        ShaderStages::COMPUTE,
        (
            if is_postprocess {
                binding_types::storage_buffer_read_only::<StrokePostprocessData>(false)
            } else {
                binding_types::storage_buffer_read_only::<ComputedPenInput>(
                    !is_postprocess && !is_bounds_eval,
                )
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
            if is_postprocess || is_bounds_eval {
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
    samples_vec: Option<&'a BufferVec<ComputedPenInput>>,
    stroke_pp_data: Option<&'a DynamicBuffer<StrokePostprocessData>>,
    stroke_pp_data_vec: Option<&'a BufferVec<StrokePostprocessData>>,
    dab_infos: Option<&'a DynamicBuffer<DabInfo>>,
    dab_infos_vec: Option<&'a BufferVec<DabInfo>>,
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
        (
            8,
            if let Some(dab_infos) = dab_infos {
                dab_infos.binding().unwrap()
            } else if let Some(dab_infos_vec) = dab_infos_vec {
                dab_infos_vec.inner_buffer().unwrap().as_entire_binding()
            } else {
                unreachable!()
            },
        ),
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
    } else if let Some(samples_vec) = samples_vec {
        entries.entries.push(BindGroupEntry {
            binding: 0,
            resource: samples_vec.inner_buffer().unwrap().as_entire_binding(),
        });
    } else if let Some(stroke_pp_data) = stroke_pp_data {
        entries.entries.push(BindGroupEntry {
            binding: 0,
            resource: stroke_pp_data.binding().unwrap(),
        });
    } else if let Some(stroke_pp_data_vec) = stroke_pp_data_vec {
        entries.entries.push(BindGroupEntry {
            binding: 0,
            resource: stroke_pp_data_vec.inner_buffer().unwrap().as_entire_binding(),
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
