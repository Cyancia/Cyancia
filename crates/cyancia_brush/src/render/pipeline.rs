use std::borrow::Cow;

use bevy_math::URect;
use cyancia_image::tile::{GpuTileInfo, GpuTileStorage, LayerBinding};
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
    ShaderSource, ShaderStages, StorageTextureAccess, TextureSampleType, TextureView,
};

use crate::render::{
    ComputedPenInput, DabInfo, InputSampler, OutputSamples, PenInput, StrokePostprocessData,
    StrokeResources, graph::CanvasResources,
};

pub struct BrushInputSamplingPipeline {
    layout: BindGroupLayout,
    pipeline: ComputePipeline,
}

impl BrushInputSamplingPipeline {
    pub fn new(
        device: &Device,
        resources: &StrokeResources,
        compiled_shader: Cow<'_, str>,
    ) -> Self {
        let mut layout_entries = BindGroupLayoutEntries::sequential(
            ShaderStages::COMPUTE,
            (
                binding_types::storage_buffer_read_only::<PenInput>(false),
                binding_types::storage_buffer::<InputSampler>(false),
                binding_types::storage_buffer::<OutputSamples>(false),
                binding_types::storage_buffer::<UVec4>(false),
                binding_types::storage_buffer::<CanvasResources>(false),
            ),
        )
        .to_vec();
        layout_entries.extend(resources.external_var_layouts.clone());
        let layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some("brush input sampling layout"),
            entries: &layout_entries,
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

        Self { layout, pipeline }
    }

    pub fn dispatch(
        &self,
        device: &Device,
        pass: &mut ComputePass,
        pen_input: &DynamicBuffer<PenInput>,
        input_sampler: &DynamicBuffer<InputSampler>,
        output_samples: &DynamicBuffer<OutputSamples>,
        bounds_eval_dispatch: &Buffer,
        resources: &StrokeResources,
    ) {
        let mut entries = BindGroupEntries::sequential((
            pen_input.binding().unwrap(),
            input_sampler.binding().unwrap(),
            output_samples.inner_buffer().unwrap().as_entire_binding(),
            bounds_eval_dispatch.as_entire_binding(),
            resources.canvas_resources.as_entire_binding(),
        ))
        .to_vec();
        entries.extend(resources.external_var_bindings());
        let bind_group = device.create_bind_group(&BindGroupDescriptor {
            label: Some("brush input sampling bind group"),
            layout: &self.layout,
            entries: &entries,
        });

        pass.push_debug_group("brush preset input sampling");
        {
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &bind_group, &[]);
            pass.dispatch_workgroups(1, 1, 1);
        }
        pass.pop_debug_group();
    }
}

#[derive(Clone)]
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
        let mut layout_entries = common_bind_group_layout_entries(resources);
        layout_entries.extend(
            DynamicBindGroupLayoutEntries::new_with_indices(
                ShaderStages::COMPUTE,
                (
                    (
                        0,
                        binding_types::storage_buffer_read_only::<ComputedPenInput>(true),
                    ),
                    (
                        5,
                        binding_types::storage_buffer_read_only::<GpuTileInfo>(false),
                    ),
                    (
                        6,
                        binding_types::texture_storage_2d_array(
                            resources.target_layer_format.wgpu_format(),
                            StorageTextureAccess::ReadOnly,
                        ),
                    ),
                    (
                        7,
                        binding_types::texture_storage_2d_array(
                            resources.target_layer_format.wgpu_format(),
                            StorageTextureAccess::WriteOnly,
                        ),
                    ),
                    (8, binding_types::storage_buffer::<DabInfo>(true)),
                ),
            )
            .to_vec(),
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
        has_selection: &Buffer,
        selection_layer_texture: &TextureView,
        selection_layer_tile_info: &Buffer,
        samples: &DynamicBuffer<ComputedPenInput>,
        samples_offsets: &[u32],
        dab_infos: &DynamicBuffer<DabInfo>,
        dab_info_offsets: &[u32],
        resources: &StrokeResources,
        intermediate_buffers: &[LayerBinding; 2],
        round: &mut u32,
    ) {
        let main_bind_group_entries = |is_even: bool| {
            let mut entries = common_bind_group_entries(
                resources,
                target_layer_texture,
                target_layer_tile_info,
                has_selection,
                selection_layer_texture,
                selection_layer_tile_info,
            );
            let (read_idx, write_idx) = if is_even { (0, 1) } else { (1, 0) };
            entries.extend(
                DynamicBindGroupEntries::new_with_indices((
                    (0, samples.binding().unwrap()),
                    (
                        5,
                        intermediate_buffers[0].tile_info_buffer.as_entire_binding(),
                    ),
                    (6, &intermediate_buffers[read_idx].texture),
                    (7, &intermediate_buffers[write_idx].texture),
                    (8, dab_infos.binding().unwrap()),
                ))
                .to_vec(),
            );
            entries
        };

        let bind_group_entries_even = main_bind_group_entries(true);
        let bind_group_even = device.create_bind_group(&BindGroupDescriptor {
            label: Some("brush main bind group even"),
            layout: &self.layout,
            entries: &bind_group_entries_even,
        });

        let bind_group_entries_odd = main_bind_group_entries(false);
        let bind_group_odd = device.create_bind_group(&BindGroupDescriptor {
            label: Some("brush main bind group odd"),
            layout: &self.layout,
            entries: &bind_group_entries_odd,
        });

        let n_tiles = intermediate_buffers[0]
            .texture
            .texture()
            .depth_or_array_layers();

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
                    n_tiles,
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

#[derive(Clone)]
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
        let mut layout_entries = common_bind_group_layout_entries(resources);
        layout_entries.extend(
            DynamicBindGroupLayoutEntries::new_with_indices(
                ShaderStages::COMPUTE,
                (
                    (
                        0,
                        binding_types::storage_buffer_read_only::<StrokePostprocessData>(false),
                    ),
                    (
                        5,
                        binding_types::storage_buffer_read_only::<GpuTileInfo>(false),
                    ),
                    (
                        6,
                        binding_types::texture_storage_2d_array(
                            resources.target_layer_format.wgpu_format(),
                            StorageTextureAccess::ReadOnly,
                        ),
                    ),
                    (
                        7,
                        binding_types::texture_storage_2d_array(
                            resources.target_layer_format.wgpu_format(),
                            StorageTextureAccess::WriteOnly,
                        ),
                    ),
                    (8, binding_types::storage_buffer::<DabInfo>(false)),
                ),
            )
            .to_vec(),
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
        has_selection: &Buffer,
        selection_layer_texture: &TextureView,
        selection_layer_tile_info: &Buffer,
        dab_info: &DynamicBuffer<DabInfo>,
        resources: &StrokeResources,
        intermediate_buffers: &[LayerBinding; 2],
        round: &mut u32,
    ) {
        let mut bind_group_entries = common_bind_group_entries(
            resources,
            target_layer_texture,
            target_layer_tile_info,
            has_selection,
            selection_layer_texture,
            selection_layer_tile_info,
        );
        let (read_idx, write_idx) = if (*round).is_multiple_of(2) {
            (0, 1)
        } else {
            (1, 0)
        };
        bind_group_entries.extend(
            DynamicBindGroupEntries::new_with_indices((
                (0, stroke_pp_data.binding().unwrap()),
                (
                    5,
                    intermediate_buffers[0].tile_info_buffer.as_entire_binding(),
                ),
                (6, &intermediate_buffers[read_idx].texture),
                (7, &intermediate_buffers[write_idx].texture),
                (8, dab_info.binding().unwrap()),
            ))
            .to_vec(),
        );
        let bind_group = device.create_bind_group(&BindGroupDescriptor {
            label: Some("brush postprocess bind group"),
            layout: &self.layout,
            entries: &bind_group_entries,
        });

        let n_tiles = intermediate_buffers[0]
            .texture
            .texture()
            .depth_or_array_layers();

        pass.push_debug_group("brush preset postprocess");
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
        let mut layout_entries = common_bind_group_layout_entries(resources);
        layout_entries.extend(
            DynamicBindGroupLayoutEntries::new_with_indices(
                ShaderStages::COMPUTE,
                (
                    (
                        0,
                        binding_types::storage_buffer_read_only::<OutputSamples>(false),
                    ),
                    (8, binding_types::storage_buffer::<DabInfo>(false)),
                ),
            )
            .to_vec(),
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
            bind_group_layouts: &[&layout],
            push_constant_ranges: &[],
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
        samples: &DynamicBuffer<OutputSamples>,
        dab_infos: &BufferVec<DabInfo>,
        target_layer_texture: &TextureView,
        target_layer_tile_info: &Buffer,
        has_selection: &Buffer,
        selection_layer_texture: &TextureView,
        selection_layer_tile_info: &Buffer,
        resources: &StrokeResources,
    ) {
        use crate::render::MAX_DABS_PER_STROKE;

        let mut bind_group_entries = common_bind_group_entries(
            resources,
            target_layer_texture,
            target_layer_tile_info,
            has_selection,
            selection_layer_texture,
            selection_layer_tile_info,
        );
        bind_group_entries.extend(
            DynamicBindGroupEntries::new_with_indices((
                (0, samples.inner_buffer().unwrap().as_entire_binding()),
                (8, dab_infos.inner_buffer().unwrap().as_entire_binding()),
            ))
            .to_vec(),
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
    }
}

#[derive(Clone)]
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
        let mut layout_entries = common_bind_group_layout_entries(resources);
        layout_entries.extend(
            DynamicBindGroupLayoutEntries::new_with_indices(
                ShaderStages::COMPUTE,
                (
                    (
                        0,
                        binding_types::storage_buffer_read_only::<StrokePostprocessData>(false),
                    ),
                    (8, binding_types::storage_buffer::<DabInfo>(false)),
                ),
            )
            .to_vec(),
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
            bind_group_layouts: &[&layout],
            push_constant_ranges: &[],
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
        stroke_pp_data: &DynamicBuffer<StrokePostprocessData>,
        target_layer_texture: &TextureView,
        target_layer_tile_info: &Buffer,
        has_selection: &Buffer,
        selection_layer_texture: &TextureView,
        selection_layer_tile_info: &Buffer,
        dab_infos: &DynamicBuffer<DabInfo>,
        resources: &StrokeResources,
    ) {
        use crate::render::MAX_DABS_PER_STROKE;

        let mut bind_group_entries = common_bind_group_entries(
            resources,
            target_layer_texture,
            target_layer_tile_info,
            has_selection,
            selection_layer_texture,
            selection_layer_tile_info,
        );
        bind_group_entries.extend(
            DynamicBindGroupEntries::new_with_indices((
                (0, stroke_pp_data.binding().unwrap()),
                (8, dab_infos.binding().unwrap()),
            ))
            .to_vec(),
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
    }
}

fn common_bind_group_layout_entries(resources: &StrokeResources) -> Vec<BindGroupLayoutEntry> {
    let mut entries = DynamicBindGroupLayoutEntries::new_with_indices(
        ShaderStages::COMPUTE,
        (
            (
                1,
                binding_types::texture_2d(TextureSampleType::Float { filterable: false }),
            ),
            (2, binding_types::storage_buffer_read_only::<URect>(false)),
            (
                3,
                binding_types::storage_buffer_read_only::<GpuTileInfo>(false),
            ),
            (
                4,
                binding_types::texture_storage_2d_array(
                    resources.target_layer_format.wgpu_format(),
                    StorageTextureAccess::ReadOnly,
                ),
            ),
            (
                9,
                binding_types::texture_storage_2d_array(
                    resources.selection_layer_format.wgpu_format(),
                    StorageTextureAccess::ReadOnly,
                ),
            ),
            (
                10,
                binding_types::storage_buffer_read_only::<GpuTileInfo>(false),
            ),
            (11, binding_types::storage_buffer_read_only::<u32>(false)),
            (
                12,
                binding_types::storage_buffer_read_only::<CanvasResources>(false),
            ),
        ),
    )
    .to_vec();
    entries.extend_from_slice(&resources.external_var_layouts);
    entries
}

fn common_bind_group_entries<'a>(
    resources: &'a StrokeResources,
    target_layer_texture: &'a TextureView,
    target_layer_tile_info: &'a Buffer,
    has_selection: &'a Buffer,
    selection_layer_texture: &'a TextureView,
    selection_layer_tile_info: &'a Buffer,
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
        (9, BindingResource::TextureView(selection_layer_texture)),
        (10, selection_layer_tile_info.as_entire_binding()),
        (11, has_selection.as_entire_binding()),
        (12, resources.canvas_resources.as_entire_binding()),
    ))
    .to_vec();

    entries.extend(resources.external_var_bindings());
    entries
}
