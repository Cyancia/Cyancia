use std::{
    collections::{HashSet, VecDeque},
    num::NonZeroU32,
    ops::Deref,
    sync::Arc,
};

use bevy_math::{IRect, Rect};
use bytemuck::Contiguous;
use cyancia_assets::{AssetAppExt, asset::AssetId, store::AssetRegistry};
use cyancia_image::{
    layer::LayerId,
    texel::{TexelDepth, TexelFormat, TexelType},
    tile::{
        DynamicLayerStorage, GpuLayerInfo, GpuTileInfo, GpuTileStorage, GpuTileStorageInner, Tile,
        TileIndex,
    },
};
use cyancia_math::number::LerpAngle;
use cyancia_render::{
    buffer::{BufferVec, DynamicBuffer},
    texture::GpuImage,
    texture_atlas::{TextureAtlas, TextureAtlasBuilder},
};
use cyancia_shader_graph::graph::{Graph, texture::TextureId};
use cyancia_utils::include_shader;
use encase::{ShaderType, StorageBuffer};
use glam::{IVec2, IVec4, UVec2, UVec3, UVec4, Vec2, Vec4Swizzles};
use gpui::App;
use parking_lot::RwLock;
use ringbuffer::{AllocRingBuffer, RingBuffer};
use uuid::Uuid;
use wesl::{VirtualResolver, Wesl};
use wgpu::{
    BindGroup, BindGroupDescriptor, BindGroupEntry, BindGroupLayout, BindGroupLayoutDescriptor,
    BindGroupLayoutEntry, BindingResource, BindingType, Buffer, BufferAddress, BufferBindingType,
    BufferDescriptor, BufferUsages, CommandEncoder, ComputePassDescriptor, ComputePipeline,
    ComputePipelineDescriptor, Device, Extent3d, MapMode, Origin3d, PipelineLayoutDescriptor,
    Queue, ShaderModuleDescriptor, ShaderSource, ShaderStages, StorageTextureAccess,
    TexelCopyTextureInfo, Texture, TextureAspect, TextureDescriptor, TextureDimension,
    TextureFormat, TextureSampleType, TextureUsages, TextureView, TextureViewDimension,
    naga::StorageAccess,
    util::{BufferInitDescriptor, DeviceExt},
    wgt::PollType,
};

use crate::{
    asset::BrushPreset,
    input_processing::{InputProcessor, RawPenInput},
    instance::{BrushPresetInstance, CompiledBrushPreset},
    render::{
        graph::{BrushGraphData, BrushGraphPostprocessData},
        pipeline::{BrushMainPipeline, BrushPostProcessPipeline},
    },
};

pub mod graph;
pub mod pipeline;

const EXTERNAL_VARIABLE_BASE_BINDING: u32 = 32;

#[derive(Debug)]
pub struct BrushStrokeSessionInfo {
    pub brush_runtime_revision: u64,
    pub target_layer_format: TexelType,
}

pub struct BrushPresetOperator {
    instance: BrushPresetInstance,
    device: Device,
    queue: Queue,
    renderer: Option<BrushPresetRenderer>,
    last_session: Option<BrushStrokeSessionInfo>,
    input_processor: InputProcessor,
    cached_brush: Option<CompiledBrushPreset>,
    intermediate_buffers: Option<[DynamicLayerStorage; 2]>,
    round: u32,
    accumulated_pixel_bounds: IRect,
}

impl BrushPresetOperator {
    pub fn new(
        instance: BrushPresetInstance,
        device: Device,
        queue: Queue,
        input_processor: InputProcessor,
    ) -> Self {
        Self {
            instance,
            renderer: None,
            device,
            queue,
            last_session: None,
            input_processor,
            cached_brush: None,
            intermediate_buffers: None,
            round: 0,
            accumulated_pixel_bounds: IRect::EMPTY,
        }
    }

    pub fn instance(&self) -> &BrushPresetInstance {
        &self.instance
    }

    pub fn instance_mut(&mut self) -> &mut BrushPresetInstance {
        &mut self.instance
    }

    pub fn begin_stroke(&mut self, input: RawPenInput, target_layer: LayerId, cx: &mut App) {
        let tiles = cx.global::<GpuTileStorage>();

        let target_layer_info = tiles.get_layer_info(target_layer).unwrap();
        let session = BrushStrokeSessionInfo {
            brush_runtime_revision: self.instance.runtime_revision(),
            target_layer_format: target_layer_info.texel_type,
        };
        match self.last_session.as_mut() {
            Some(last_session) => {
                if last_session.brush_runtime_revision != session.brush_runtime_revision {
                    self.cached_brush = None;
                    self.renderer = None;
                }

                if last_session.target_layer_format != session.target_layer_format {
                    self.renderer = None;
                }

                self.last_session = Some(session);
            }
            None => {
                self.last_session = Some(session);
            }
        }

        let compiled_brush = self.cached_brush.get_or_insert_with(|| {
            let now = std::time::Instant::now();
            let compiled = self
                .instance
                .compile(EXTERNAL_VARIABLE_BASE_BINDING, cx)
                .unwrap();
            log::info!("Brush preset compilation: {:?}", now.elapsed());
            println!("Compiled brush preset: {}", compiled);
            compiled
        });

        let renderer = self.renderer.get_or_insert_with(|| {
            let now = std::time::Instant::now();
            let renderer = BrushPresetRenderer::new(
                &self.device,
                &self.queue,
                &compiled_brush,
                target_layer,
                target_layer_info.texel_type,
                cx.assets(),
            );
            log::info!("Brush preset renderer creation: {:?}", now.elapsed());
            renderer
        });

        let intermediate_buffers = self.intermediate_buffers.insert([
            DynamicLayerStorage::new(
                self.device.clone().into(),
                self.queue.clone().into(),
                target_layer_info,
            ),
            DynamicLayerStorage::new(
                self.device.clone().into(),
                self.queue.clone().into(),
                target_layer_info,
            ),
        ]);
        self.round = 0;
        self.accumulated_pixel_bounds = IRect::EMPTY;
        self.input_processor.reset();

        renderer.update(
            &self.device,
            &self.queue,
            self.input_processor
                .push(input, self.instance.required_spacing_graph().read(cx), cx),
            self.instance.main_graph().read(cx),
            cx.global::<GpuTileStorage>(),
            intermediate_buffers,
            &mut self.round,
            &mut self.accumulated_pixel_bounds,
            cx,
        );
    }

    pub fn update_stroke(&mut self, input: RawPenInput, cx: &mut App) {
        let Some(renderer) = &mut self.renderer else {
            return;
        };

        let Some(intermediate_buffers) = &mut self.intermediate_buffers else {
            return;
        };

        renderer.update(
            &self.device,
            &self.queue,
            self.input_processor
                .push(input, self.instance.required_spacing_graph().read(cx), cx),
            self.instance.main_graph().read(cx),
            cx.global::<GpuTileStorage>(),
            intermediate_buffers,
            &mut self.round,
            &mut self.accumulated_pixel_bounds,
            cx,
        );
    }

    pub fn end_stroke(
        &mut self,
        final_input: RawPenInput,
        target_layer: LayerId,
        cx: &mut App,
    ) -> Option<IRect> {
        let renderer = self.renderer.as_mut()?;
        let intermediate_buffers = self.intermediate_buffers.as_mut()?;

        renderer.update(
            &self.device,
            &self.queue,
            self.input_processor
                .flush(final_input, self.instance.required_spacing_graph().read(cx), cx),
            self.instance.main_graph().read(cx),
            cx.global::<GpuTileStorage>(),
            intermediate_buffers,
            &mut self.round,
            &mut self.accumulated_pixel_bounds,
            cx,
        );

        let now = std::time::Instant::now();
        renderer.postprocess_stroke(
            &self.device,
            &self.queue,
            self.instance
                .stroke_postprocess_graphs()
                .iter()
                .map(|g| g.read(cx)),
            cx.global::<GpuTileStorage>(),
            final_input.time,
            intermediate_buffers,
            &mut self.round,
            &mut self.accumulated_pixel_bounds,
            cx,
        );
        renderer.copy_last_surface_to_layer(
            &self.device,
            &self.queue,
            cx.global::<GpuTileStorage>(),
            target_layer,
            intermediate_buffers,
            self.round,
        );
        log::info!("Brush stroke postprocess and copy: {:?}", now.elapsed());

        Some(self.accumulated_pixel_bounds)
    }

    pub fn generate_preview(&mut self, cx: &mut App) -> Option<(IRect, DynamicLayerStorage)> {
        let renderer = self.renderer.as_mut()?;
        let mut accumulated_pixel_bounds = self.accumulated_pixel_bounds;
        let mut round = self.round;

        let intermediate_buffers = self.intermediate_buffers.as_ref()?;
        let result_buffer = intermediate_buffers[self.round as usize % 2].deep_clone();
        let another_buffer = intermediate_buffers[(self.round as usize + 1) % 2].deep_clone();

        let mut new_intermediate_buffers = if round % 2 == 0 {
            [result_buffer, another_buffer]
        } else {
            [another_buffer, result_buffer]
        };

        renderer.postprocess_stroke(
            &self.device,
            &self.queue,
            self.instance
                .stroke_postprocess_graphs()
                .iter()
                .map(|g| g.read(cx)),
            cx.global::<GpuTileStorage>(),
            // TODO
            Time {
                now: 0.0,
                stroke_begin: 0.0,
            },
            &mut new_intermediate_buffers,
            &mut round,
            &mut accumulated_pixel_bounds,
            cx,
        );

        let [result_buffer_a, result_buffer_b] = new_intermediate_buffers;
        if round % 2 == 0 {
            Some((accumulated_pixel_bounds, result_buffer_a))
        } else {
            Some((accumulated_pixel_bounds, result_buffer_b))
        }
    }
}

pub struct BrushPresetRenderer {
    main: BrushMainPipeline,
    stroke_pp: Vec<BrushPostProcessPipeline>,
    resources: StrokeResources,
    samples_buffer: DynamicBuffer<ComputedPenInput>,
    samples_offsets: Vec<u32>,
    stroke_pp_data: DynamicBuffer<StrokePostprocessData>,
    dab_info_buffer: DynamicBuffer<DabInfo>,
    dab_info_offsets: Vec<u32>,
}

impl BrushPresetRenderer {
    pub fn new(
        device: &Device,
        queue: &Queue,
        brush: &CompiledBrushPreset,
        target_layer_id: LayerId,
        target_layer_format: TexelType,
        assets: &AssetRegistry,
    ) -> Self {
        let resources = StrokeResources::new(
            device,
            queue,
            &brush,
            target_layer_id,
            target_layer_format,
            assets,
        );

        let main = BrushMainPipeline::new(device, &resources, brush.main_graph.clone().into());

        let mut stroke_pp = Vec::new();
        for graph in &brush.stroke_postprocess_graphs {
            let main = BrushPostProcessPipeline::new(device, &resources, graph.clone().into());
            stroke_pp.push(main);
        }

        Self {
            main,
            stroke_pp,
            resources,
            samples_buffer: DynamicBuffer::new("samples buffer".into(), BufferUsages::STORAGE),
            samples_offsets: Vec::new(),
            stroke_pp_data: DynamicBuffer::new(
                "stroke postprocess data buffer".into(),
                BufferUsages::STORAGE,
            ),
            dab_info_buffer: DynamicBuffer::new("dab info buffer".into(), BufferUsages::STORAGE),
            dab_info_offsets: Vec::new(),
        }
    }

    // TODO: Copy unchanged tiles onto another buffer?
    pub fn update(
        &mut self,
        device: &Device,
        queue: &Queue,
        pen_input: Vec<ComputedPenInput>,
        main_graph: &Graph<BrushGraphData>,
        tiles: &GpuTileStorage,
        intermediate_buffers: &mut [DynamicLayerStorage; 2],
        round: &mut u32,
        accumulated_pixel_bounds: &mut IRect,
        cx: &App,
    ) {
        if pen_input.is_empty() {
            return;
        }

        self.samples_buffer.clear();
        self.dab_info_buffer.clear();
        self.samples_offsets.clear();
        self.dab_info_offsets.clear();

        for sample in pen_input {
            let output = main_graph
                .run(&BrushGraphData { pen_input: sample }, Vec::new(), cx)
                .unwrap();

            assert_eq!(output.len(), 1);
            let bounds =
                GpuTileStorageInner::snap_to_tile_grid(output[0].as_ref::<Rect>().as_irect());
            self.dab_info_offsets
                .push(self.dab_info_buffer.push(&DabInfo {
                    bound_min: bounds.min,
                    bound_max: bounds.max,
                }) as u32);
            self.samples_offsets
                .push(self.samples_buffer.push(&sample) as u32);

            let size = bounds.size().as_uvec2();

            intermediate_buffers[0].ensure_pixel_area(bounds);
            intermediate_buffers[1].ensure_pixel_area(bounds);

            *accumulated_pixel_bounds = accumulated_pixel_bounds.union(bounds);
        }

        self.samples_buffer.write_buffer(device, queue);
        self.dab_info_buffer.write_buffer(device, queue);
        let target_layer = tiles
            .get_layer_binding_or_empty(self.resources.target_layer_id)
            .unwrap();

        let mut ec = device.create_command_encoder(&Default::default());

        ec.push_debug_group("brush preset update stroke");
        {
            let mut pass = ec.begin_compute_pass(&ComputePassDescriptor {
                label: Some("brush preset update pass"),
                ..Default::default()
            });
            self.main.dispatch(
                device,
                &mut pass,
                target_layer.texture.as_ref(),
                &target_layer.tile_info_buffer,
                &self.samples_buffer,
                &self.samples_offsets,
                &self.dab_info_buffer,
                &self.dab_info_offsets,
                &self.resources,
                &intermediate_buffers,
                round,
            );
        }
        ec.pop_debug_group();

        queue.submit([ec.finish()]);
    }

    pub fn postprocess_stroke<'a>(
        &mut self,
        device: &Device,
        queue: &Queue,
        graphs: impl Iterator<Item = &'a Graph<BrushGraphPostprocessData>>,
        tiles: &GpuTileStorage,
        time: Time,
        intermediate_buffers: &mut [DynamicLayerStorage; 2],
        round: &mut u32,
        accumulated_pixel_bounds: &mut IRect,
        cx: &App,
    ) {
        if accumulated_pixel_bounds.is_empty() {
            return;
        }

        let target_layer = tiles
            .get_layer_binding_or_empty(self.resources.target_layer_id)
            .unwrap();

        let mut ec = device.create_command_encoder(&Default::default());

        ec.push_debug_group("brush preset stroke postprocess");
        {
            let mut pass = ec.begin_compute_pass(&ComputePassDescriptor {
                label: Some("brush preset stroke postprocess pass"),
                ..Default::default()
            });

            for (graph, pipeline) in graphs.into_iter().zip(&self.stroke_pp) {
                let output = graph
                    .run(
                        &BrushGraphPostprocessData {
                            accumulated_pixel_bounds: *accumulated_pixel_bounds,
                            time: Time {
                                now: 0.0,          // TODO
                                stroke_begin: 0.0, // TODO
                            },
                        },
                        Vec::new(),
                        cx,
                    )
                    .unwrap();
                assert_eq!(output.len(), 1);
                let bounds =
                    GpuTileStorageInner::snap_to_tile_grid(output[0].as_ref::<Rect>().as_irect());
                self.dab_info_buffer.clear();
                self.dab_info_buffer.push(&DabInfo {
                    bound_min: bounds.min,
                    bound_max: bounds.max,
                });
                self.dab_info_buffer.write_buffer(device, queue);

                self.stroke_pp_data.clear();
                self.stroke_pp_data.push(&StrokePostprocessData {
                    accumulated_pixel_bounds: *accumulated_pixel_bounds,
                    time,
                });
                self.stroke_pp_data.write_buffer(device, queue);

                *accumulated_pixel_bounds = accumulated_pixel_bounds.union(bounds);
                pipeline.dispatch(
                    device,
                    &mut pass,
                    &self.stroke_pp_data,
                    target_layer.texture.as_ref(),
                    &target_layer.tile_info_buffer,
                    &self.dab_info_buffer,
                    &self.resources,
                    &intermediate_buffers,
                    round,
                );
            }
        }
        ec.pop_debug_group();

        queue.submit([ec.finish()]);
    }

    pub fn copy_last_surface_to_layer(
        &self,
        device: &Device,
        queue: &Queue,
        tiles: &GpuTileStorage,
        target_layer_id: LayerId,
        intermediate_buffers: &[DynamicLayerStorage; 2],
        round: u32,
    ) {
        let mut target_layer = tiles.get_layer_mut(target_layer_id).unwrap();
        let result_buffer = &intermediate_buffers[round as usize % 2];

        for (coord, _, _) in result_buffer.iter_tiles() {
            target_layer.get_tile_or_allocate(coord);
        }

        let mut ec = device.create_command_encoder(&Default::default());
        ec.push_debug_group("copy brush preset result to target layer");
        for (coord, src, _) in result_buffer.iter_tiles() {
            let dst = target_layer.get_tile_layer(coord).unwrap();

            ec.copy_texture_to_texture(
                TexelCopyTextureInfo {
                    texture: result_buffer.texture().unwrap().texture(),
                    mip_level: 0,
                    origin: Origin3d { x: 0, y: 0, z: src },
                    aspect: TextureAspect::All,
                },
                TexelCopyTextureInfo {
                    texture: target_layer.texture().unwrap().texture(),
                    mip_level: 0,
                    origin: Origin3d { x: 0, y: 0, z: dst },
                    aspect: TextureAspect::All,
                },
                Extent3d {
                    width: GpuTileStorageInner::TILE_SIZE,
                    height: GpuTileStorageInner::TILE_SIZE,
                    depth_or_array_layers: 1,
                },
            );
        }
        ec.pop_debug_group();
        queue.submit([ec.finish()]);

        log::info!(
            "Copied tiles {:?} from intermediate buffer to target layer",
            result_buffer
                .iter_tiles()
                .map(|(index, _, _)| index)
                .collect::<Vec<_>>()
        );
    }
}

#[derive(ShaderType, Debug, Default, Clone, Copy)]
pub struct ComputedPenInput {
    pub position: Vec2,
    pub draw_direction_vec: Vec2,
    pub draw_direction_angle: f32,
    pub time: Time,
}

#[derive(ShaderType, Debug, Default, Clone, Copy)]
pub struct StrokePostprocessData {
    pub accumulated_pixel_bounds: IRect,
    pub time: Time,
}

#[derive(ShaderType, Debug, Default, Clone, Copy)]
pub struct Time {
    pub now: f32,
    pub stroke_begin: f32,
}

#[derive(ShaderType, Debug, Default, Clone, Copy)]
pub struct DabInfo {
    pub bound_min: IVec2,
    pub bound_max: IVec2,
}

pub struct StrokeResources {
    pub external_var_layouts: Vec<BindGroupLayoutEntry>,
    pub external_var_buffers: Vec<Buffer>,
    pub referenced_textures: TextureAtlas,

    pub target_layer_id: LayerId,
    pub target_layer_format: TexelType,
}

impl StrokeResources {
    fn new(
        device: &Device,
        queue: &Queue,
        brush: &CompiledBrushPreset,
        target_layer_id: LayerId,
        target_layer_format: TexelType,
        assets: &AssetRegistry,
    ) -> Self {
        let mut external_var_layouts = Vec::new();
        let mut cur_binding = EXTERNAL_VARIABLE_BASE_BINDING;
        for _ in 0..brush.external_vars.all().len() {
            external_var_layouts.push(BindGroupLayoutEntry {
                binding: cur_binding,
                visibility: ShaderStages::COMPUTE,
                ty: BindingType::Buffer {
                    ty: BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            });
            cur_binding += 1;
        }

        let mut external_var_buffers = Vec::new();
        for var in brush.external_vars.all().iter() {
            let buffer = var.value.try_write_into_shader_buffer().unwrap();
            let gpu_buffer = device.create_buffer_init(&BufferInitDescriptor {
                label: Some("external variable buffer"),
                contents: &buffer,
                usage: BufferUsages::STORAGE,
            });
            external_var_buffers.push(gpu_buffer);
        }

        let empty_texture = device.create_texture(&TextureDescriptor {
            label: Some("empty texture"),
            size: Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: TextureDimension::D2,
            format: TextureFormat::Rgba8Unorm,
            usage: TextureUsages::TEXTURE_BINDING
                | TextureUsages::STORAGE_BINDING
                | TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let mut referenced_textures_builder =
            TextureAtlasBuilder::with_capacity(brush.texture_usage.len());
        for id in &brush.texture_usage {
            if *id == TextureId::NULL {
                referenced_textures_builder.add_texture(empty_texture.clone());
                continue;
            }

            let handle = assets.handle(AssetId::new(**id)).unwrap();
            let gpu_image = GpuImage::from_asset(
                &device,
                &queue,
                &handle.get().unwrap(),
                // TODO: This is weird but, adding TEXTURE_BINDING usage to avoid vulkan validation error:
                // VALIDATION [VUID-VkImageViewCreateInfo-image-04441 (0xb75da543)]
                // vkCreateImageView(): pCreateInfo->image (VkImage 0xb550000000b55) was created with VK_IMAGE_USAGE_TRANSFER_SRC_BIT|VK_IMAGE_USAGE_TRANSFER_DST_BIT but requires VK_IMAGE_USAGE_SAMPLED_BIT|VK_IMAGE_USAGE_STORAGE_BIT|VK_IMAGE_USAGE_COLOR_ATTACHMENT_BIT|VK_IMAGE_USAGE_DEPTH_STENCIL_ATTACHMENT_BIT|VK_IMAGE_USAGE_TRANSIENT_ATTACHMENT_BIT|VK_IMAGE_USAGE_INPUT_ATTACHMENT_BIT|VK_IMAGE_USAGE_FRAGMENT_SHADING_RATE_ATTACHMENT_BIT_KHR|VK_IMAGE_USAGE_FRAGMENT_DENSITY_MAP_BIT_EXT|VK_IMAGE_USAGE_VIDEO_DECODE_DST_BIT_KHR|VK_IMAGE_USAGE_VIDEO_DECODE_DPB_BIT_KHR|VK_IMAGE_USAGE_VIDEO_ENCODE_SRC_BIT_KHR|VK_IMAGE_USAGE_VIDEO_ENCODE_DPB_BIT_KHR|VK_IMAGE_USAGE_SAMPLE_WEIGHT_BIT_QCOM|VK_IMAGE_USAGE_SAMPLE_BLOCK_MATCH_BIT_QCOM|VK_IMAGE_USAGE_VIDEO_ENCODE_QUANTIZATION_DELTA_MAP_BIT_KHR|VK_IMAGE_USAGE_VIDEO_ENCODE_EMPHASIS_MAP_BIT_KHR.
                // The Vulkan spec states: image must have been created with a usage value containing at least one of the following: VK_IMAGE_USAGE_SAMPLED_BIT VK_IMAGE_USAGE_STORAGE_BIT VK_IMAGE_USAGE_COLOR_ATTACHMENT_BIT VK_IMAGE_USAGE_DEPTH_STENCIL_ATTACHMENT_BIT VK_IMAGE_USAGE_INPUT_ATTACHMENT_BIT VK_IMAGE_USAGE_TRANSIENT_ATTACHMENT_BIT VK_IMAGE_USAGE_FRAGMENT_SHADING_RATE_ATTACHMENT_BIT_KHR VK_IMAGE_USAGE_FRAGMENT_DENSITY_MAP_BIT_EXT VK_IMAGE_USAGE_VIDEO_DECODE_DST_BIT_KHR VK_IMAGE_USAGE_VIDEO_DECODE_DPB_BIT_KHR VK_IMAGE_USAGE_VIDEO_ENCODE_SRC_BIT_KHR VK_IMAGE_USAGE_VIDEO_ENCODE_DPB_BIT_KHR VK_IMAGE_USAGE_SAMPLE_WEIGHT_BIT_QCOM VK_IMAGE_USAGE_SAMPLE_BLOCK_MATCH_BIT_QCOM VK_IMAGE_USAGE_VIDEO_ENCODE_QUANTIZATION_DELTA_MAP_BIT_KHR VK_IMAGE_USAGE_VIDEO_ENCODE_EMPHASIS_MAP_BIT_KHR (https://docs.vulkan.org/spec/latest/chapters/resources.html#VUID-VkImageViewCreateInfo-image-04441)
                TextureUsages::COPY_SRC | TextureUsages::TEXTURE_BINDING,
            );
            referenced_textures_builder.add_texture(gpu_image.texture.clone());
        }
        if referenced_textures_builder.is_empty() {
            referenced_textures_builder.add_texture(empty_texture.clone());
        }
        let referenced_textures = referenced_textures_builder
            .build(Some("referenced textures"), device, queue)
            .unwrap();

        Self {
            external_var_layouts,
            external_var_buffers,
            referenced_textures,

            target_layer_id,
            target_layer_format,
        }
    }

    fn external_var_bindings(&self) -> Vec<BindGroupEntry<'_>> {
        self.external_var_buffers
            .iter()
            .enumerate()
            .map(|(i, buffer)| BindGroupEntry {
                binding: EXTERNAL_VARIABLE_BASE_BINDING + i as u32,
                resource: BindingResource::Buffer(buffer.as_entire_buffer_binding()),
            })
            .collect()
    }
}
