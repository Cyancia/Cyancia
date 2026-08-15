use anyhow::{Result, anyhow};
use bevy_math::IRect;
use futures::channel::oneshot;
use glam::IVec2;
use iced_runtime::Task;
use lapiz_assets::{AssetAppExt, store::AssetRegistry};
use lapiz_canvas::{CanvasAppExt, CanvasId};
use lapiz_image::{
    layer::LayerId,
    scan_pixels::ScanPixelsPipeline,
    texel::{TexelDepth, TexelFormat, TexelType},
    tile::{DynamicLayerStorage, GpuLayerInfo, GpuTileStorage, LayerBinding, TileStorageAppExt},
};
use lapiz_render::{
    readback::{
        create_readback_buffer_and_schedule_copy_buffer, readback_buffer_raw_on_submit_async,
    },
    render_context::RenderContextAppExt,
    texture::GpuImage,
    texture_atlas::{TextureAtlas, TextureAtlasBuilder},
};
use lapiz_runtime::Services;
use lapiz_shader_graph::graph::external::GraphExternalVariableStorage;
use parking_lot::Mutex;
use std::{
    borrow::Cow,
    collections::{HashMap, HashSet},
    num::NonZeroU64,
    sync::Arc,
};
use uuid::Uuid;
use wgpu::{
    BindGroupEntry, BindGroupLayoutEntry, BindingResource, BindingType, Buffer, BufferBindingType,
    BufferDescriptor, BufferUsages, ComputePassDescriptor, Device, Extent3d, Queue, ShaderStages,
    TextureDescriptor, TextureDimension, TextureFormat, TextureUsages,
};

use crate::{
    asset::{FilterGroupId, FilterSlotRef},
    instance::CompiledFilterPreset,
    render::pipeline::{FilterBoundsEvalPipeline, FilterMainPipeline},
};
pub mod graph;
pub mod pipeline;

pub(crate) const EXTERNAL_VARIABLE_BASE_BINDING: u32 = 32;

fn rgba8_texel_type() -> TexelType {
    TexelType {
        format: TexelFormat::Rgba,
        depth: TexelDepth::Bit8,
    }
}
fn alpha8_texel_type() -> TexelType {
    TexelType {
        format: TexelFormat::Alpha,
        depth: TexelDepth::Bit8,
    }
}

#[derive(Clone)]
pub struct FilterResources {
    pub external_var_storage: Arc<GraphExternalVariableStorage>,
    pub external_var_layouts: Vec<BindGroupLayoutEntry>,
    pub external_var_buffers: Vec<Buffer>,
    pub texture_atlas: TextureAtlas,
    pub target_layer_format: TexelType,
    pub selection_layer_format: TexelType,
}
impl FilterResources {
    fn new(
        device: &Device,
        queue: &Queue,
        compiled: &CompiledFilterPreset,
        assets: &AssetRegistry,
    ) -> Result<Self> {
        let target_layer_format = rgba8_texel_type();
        let selection_layer_format = alpha8_texel_type();
        // Share the exact storage with the FilterInstance so external variable
        // edits are visible here; buffer/binding order therefore also matches
        // the order used by `FilterInstance::compile`.
        let external_var_storage = compiled.external_vars.clone();
        let external_var_layouts = (EXTERNAL_VARIABLE_BASE_BINDING..)
            .take(external_var_storage.all().len())
            .map(|binding| BindGroupLayoutEntry {
                binding,
                visibility: ShaderStages::COMPUTE,
                ty: BindingType::Buffer {
                    ty: BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            })
            .collect();
        let external_var_buffers = external_var_storage
            .all()
            .iter()
            .map(|entry| {
                let var = entry.value();
                let (_, size) = var.value.ty().wgsl_type().expect("ext var wgsl type");
                let gpu_buffer = device.create_buffer(&BufferDescriptor {
                    label: Some("filter external variable buffer"),
                    size,
                    usage: BufferUsages::STORAGE | BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                });
                let mut writer = queue
                    .write_buffer_with(&gpu_buffer, 0, NonZeroU64::new(size).unwrap())
                    .unwrap();
                var.value.try_write_into_shader_buffer(&mut writer).unwrap();
                gpu_buffer
            })
            .collect();

        // Texture atlas for the builtin TextureNode / GetPixelColorNode /
        // TextureSizeNode nodes (same approach as the brush renderer).
        let used_textures = compiled.texture_usage.used_textures_ordered();
        let mut texture_atlas_builder = TextureAtlasBuilder::with_capacity(used_textures.len());
        let empty_texture = device.create_texture(&TextureDescriptor {
            label: Some("filter empty texture"),
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
        for texture_id in used_textures {
            let texture = match *texture_id {
                Some(asset_id) => assets
                    .handle(asset_id)
                    .ok()
                    .and_then(|handle| handle.get().ok())
                    .map(|asset| {
                        GpuImage::from_asset(
                            device,
                            queue,
                            &asset,
                            TextureUsages::COPY_SRC | TextureUsages::TEXTURE_BINDING,
                        )
                        .texture
                    })
                    .unwrap_or_else(|| empty_texture.clone()),
                None => empty_texture.clone(),
            };
            texture_atlas_builder.add_texture(texture);
        }
        if texture_atlas_builder.is_empty() {
            texture_atlas_builder.add_texture(empty_texture.clone());
        }
        let texture_atlas = texture_atlas_builder
            .build(Some("filter texture atlas"), device, queue)
            .ok_or_else(|| anyhow!("Failed to build filter texture atlas"))?;

        Ok(Self {
            external_var_storage,
            external_var_layouts,
            external_var_buffers,
            texture_atlas,
            target_layer_format,
            selection_layer_format,
        })
    }

    /// Queue writes the current external variable values. This only needs
    /// `&self`: the buffers themselves are immutable GPU handles.
    pub fn update_external_var_buffers(&self, queue: &Queue) {
        for (ext_var, var_buffer) in self
            .external_var_storage
            .all()
            .iter()
            .zip(&self.external_var_buffers)
        {
            let mut writer = queue
                .write_buffer_with(var_buffer, 0, NonZeroU64::new(var_buffer.size()).unwrap())
                .unwrap();
            ext_var
                .value
                .try_write_into_shader_buffer(&mut writer)
                .unwrap();
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

pub struct FilterRenderer {
    device: Device,
    queue: Queue,
    main_pipelines: Vec<FilterMainPipeline>,
    bounds_eval_pipelines: Vec<FilterBoundsEvalPipeline>,
    resources: FilterResources,
    scan_pixels: ScanPixelsPipeline,
    chain: Arc<Mutex<Option<oneshot::Receiver<()>>>>,
    /// Per-group (input, output) wiring in topological order.
    wiring: Vec<(FilterSlotRef, FilterSlotRef)>,
    /// Group ids in topological order (keys for intermediate buffers).
    group_ids: Vec<FilterGroupId>,
}
impl FilterRenderer {
    #[tracing::instrument(skip_all, name = "new_filter_renderer")]
    pub fn new(services: &Services, compiled: CompiledFilterPreset) -> Result<Self> {
        let device = services.render_device().clone();
        let queue = services.render_queue().clone();
        let resources = FilterResources::new(&device, &queue, &compiled, services.assets())?;
        let scan_pixels = ScanPixelsPipeline::new(&device, resources.selection_layer_format);
        let mut main_pipelines = Vec::with_capacity(compiled.groups.len());
        let mut bounds_eval_pipelines = Vec::with_capacity(compiled.groups.len());
        for group in &compiled.groups {
            main_pipelines.push(FilterMainPipeline::new(
                &device,
                &resources,
                Cow::Owned(group.main.clone()),
            ));
            bounds_eval_pipelines.push(FilterBoundsEvalPipeline::new(
                &device,
                &resources,
                Cow::Owned(group.bounds_eval.clone()),
            ));
        }
        let wiring = compiled
            .groups
            .iter()
            .map(|g| (g.input, g.output))
            .collect();
        let group_ids = compiled.groups.iter().map(|g| g.id).collect();
        let (tx, rx) = oneshot::channel();
        tx.send(()).ok();
        Ok(Self {
            device,
            queue,
            main_pipelines,
            bounds_eval_pipelines,
            resources,
            scan_pixels,
            chain: Arc::new(Mutex::new(Some(rx))),
            wiring,
            group_ids,
        })
    }

    pub fn run(
        &self,
        services: &Services,
        canvas_id: CanvasId,
        layer_ids: Vec<LayerId>,
    ) -> Task<Result<HashMap<LayerId, DynamicLayerStorage>>> {
        // The instance edits the shared external variable storage; re-upload the
        // current values before this render round (same queue guarantees ordering
        // ahead of the worker's submits).
        self.resources.update_external_var_buffers(&self.queue);

        let Some(canvas) = services.canvas(&canvas_id) else {
            return Task::done(Err(anyhow!(
                "Filter render failed: canvas no longer exists"
            )));
        };
        let tile_storage = services.tile_storage();
        let selection_layer_id = canvas.image.selection_layer();
        let Some(selection) = tile_storage.get_layer_binding_or_empty(selection_layer_id) else {
            return Task::done(Err(anyhow!(
                "Filter render failed: selection layer binding unavailable"
            )));
        };
        let image_tile_rect = canvas.image.image_tile_rect();
        let mut layer_data: Vec<(LayerId, LayerBinding, Vec<IVec2>)> = Vec::new();
        for &layer_id in &layer_ids {
            let Some(binding) = tile_storage.get_layer_binding_or_empty(layer_id) else {
                return Task::done(Err(anyhow!(
                    "Filter render failed: target layer binding unavailable"
                )));
            };
            let layer_tiles = tile_storage.get_layer_tiles(layer_id).unwrap_or_default();
            layer_data.push((layer_id, binding, layer_tiles));
        }
        let device = self.device.clone();
        let queue = self.queue.clone();
        let main_pipelines = self.main_pipelines.clone();
        let bounds_eval_pipelines = self.bounds_eval_pipelines.clone();
        let resources = self.resources.clone();
        let scan_pixels = self.scan_pixels.clone();
        let wiring = self.wiring.clone();
        let group_ids = self.group_ids.clone();
        let (new_tx, new_rx) = oneshot::channel();
        let prev = {
            let mut guard = self.chain.lock();
            (*guard).replace(new_rx).unwrap_or_else(|| {
                let (t, r) = oneshot::channel();
                t.send(()).ok();
                r
            })
        };
        Task::future(async move {
            let _ = prev.await;
            let result = filter_worker(
                &device,
                &queue,
                &main_pipelines,
                &bounds_eval_pipelines,
                &resources,
                &scan_pixels,
                &selection,
                image_tile_rect,
                &layer_data,
                &wiring,
                &group_ids,
            )
            .await;
            new_tx.send(()).ok();
            result
        })
    }
}

async fn filter_worker(
    device: &Device,
    queue: &Queue,
    main_pipelines: &[FilterMainPipeline],
    bounds_eval_pipelines: &[FilterBoundsEvalPipeline],
    resources: &FilterResources,
    scan_pixels: &ScanPixelsPipeline,
    selection: &LayerBinding,
    image_tile_rect: IRect,
    layer_data: &[(LayerId, LayerBinding, Vec<IVec2>)],
    wiring: &[(FilterSlotRef, FilterSlotRef)],
    group_ids: &[FilterGroupId],
) -> Result<HashMap<LayerId, DynamicLayerStorage>> {
    let mut results = HashMap::new();
    for (layer_id, target_layer, layer_tiles) in layer_data {
        let has_selection = scan_pixels.scan_to_binary_buffer(device, queue, selection);
        let result = run_filter_on_layer(
            device,
            queue,
            main_pipelines,
            bounds_eval_pipelines,
            resources,
            target_layer,
            selection,
            &has_selection,
            image_tile_rect,
            layer_tiles,
            wiring,
            group_ids,
        )
        .await?;
        results.insert(*layer_id, result);
    }
    Ok(results)
}

#[allow(clippy::too_many_arguments)]
async fn run_filter_on_layer(
    device: &Device,
    queue: &Queue,
    main_pipelines: &[FilterMainPipeline],
    bounds_eval_pipelines: &[FilterBoundsEvalPipeline],
    resources: &FilterResources,
    target_layer: &LayerBinding,
    selection: &LayerBinding,
    has_selection: &Buffer,
    image_tile_rect: IRect,
    layer_tiles: &[IVec2],
    wiring: &[(FilterSlotRef, FilterSlotRef)],
    group_ids: &[FilterGroupId],
) -> Result<DynamicLayerStorage> {
    let mut r0_tile_rect = image_tile_rect;
    for t in layer_tiles {
        r0_tile_rect = r0_tile_rect.union(IRect {
            min: *t,
            max: *t + IVec2::ONE,
        });
    }
    let bounds0 = GpuTileStorage::tile_rect_to_pixel(r0_tile_rect);
    let n_groups = main_pipelines.len();
    if n_groups == 0 {
        return Err(anyhow!("Filter preset contains no shader groups"));
    }
    if wiring.len() != n_groups || group_ids.len() != n_groups {
        return Err(anyhow!("Filter preset wiring/group id count mismatch"));
    }
    // Index of the unique group whose output goes directly to the layer.
    let final_index = wiring
        .iter()
        .position(|(_, output)| *output == FilterSlotRef::Layer)
        .ok_or_else(|| anyhow!("Filter preset has no group with output == Layer"))?;
    let final_group_uuid = group_ids[final_index].0;
    let mut outputs: HashMap<Uuid, DynamicLayerStorage> = HashMap::new();
    let mut out_bounds: HashMap<Uuid, IRect> = HashMap::new();

    for i in 0..n_groups {
        let (input_ref, _output_ref) = wiring[i];
        let group_uuid = group_ids[i].0;
        // Input comes from the layer or from the producing group's intermediate buffer.
        let (input_binding, in_bounds) = match input_ref {
            FilterSlotRef::Layer => (target_layer.clone(), bounds0),
            FilterSlotRef::Group(producer) => {
                let producer_binding = outputs
                    .get(&producer)
                    .ok_or_else(|| {
                        anyhow!("Filter group input references unknown or not-yet-produced group")
                    })?
                    .binding_or_empty();
                let producer_bounds = *out_bounds
                    .get(&producer)
                    .ok_or_else(|| anyhow!("Filter group input bounds missing"))?;
                (producer_binding, producer_bounds)
            }
        };

        let bounds_input = write_bounds_buffer(device, queue, in_bounds);
        let out_bounds_i = run_bounds_eval(
            device,
            queue,
            &bounds_eval_pipelines[i],
            resources,
            &input_binding,
            selection,
            has_selection,
            in_bounds,
        )
        .await;
        let out_tile_rect = GpuTileStorage::pixel_rect_to_tile(out_bounds_i);
        let mut output = DynamicLayerStorage::new(
            device.clone(),
            queue.clone(),
            GpuLayerInfo {
                texel_type: resources.target_layer_format,
            },
        );
        if !out_tile_rect.is_empty() {
            output.allocate_tiles(out_tile_rect);
        }
        if !output.is_empty() {
            let mut ec = device.create_command_encoder(&Default::default());
            {
                let mut pass = ec.begin_compute_pass(&ComputePassDescriptor {
                    label: Some("filter main pass"),
                    ..Default::default()
                });
                main_pipelines[i].dispatch(
                    device,
                    &mut pass,
                    &input_binding,
                    &output,
                    selection,
                    has_selection,
                    &bounds_input,
                    target_layer,
                    resources,
                );
            }
            queue.submit([ec.finish()]);
        }

        outputs.insert(group_uuid, output);
        out_bounds.insert(group_uuid, out_bounds_i);

        // Release intermediates no longer consumed by any remaining group. The
        // final group's output is always kept so it can be returned below.
        let needed: HashSet<Uuid> = wiring[i + 1..]
            .iter()
            .filter_map(|(input_ref, _)| match input_ref {
                FilterSlotRef::Group(producer) => Some(*producer),
                FilterSlotRef::Layer => None,
            })
            .collect();
        outputs.retain(|uuid, _| needed.contains(uuid) || *uuid == final_group_uuid);
        out_bounds.retain(|uuid, _| needed.contains(uuid));
    }

    outputs
        .remove(&final_group_uuid)
        .ok_or_else(|| anyhow!("Final filter group output is missing"))
}

fn write_bounds_buffer(device: &Device, queue: &Queue, rect: IRect) -> Buffer {
    let mut bytes = Vec::with_capacity(16);
    for value in [rect.min.x, rect.min.y, rect.max.x, rect.max.y] {
        bytes.extend_from_slice(&value.to_ne_bytes());
    }
    let buffer = device.create_buffer(&BufferDescriptor {
        label: Some("filter bounds input"),
        size: 16,
        usage: BufferUsages::STORAGE | BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    queue.write_buffer(&buffer, 0, &bytes);
    buffer
}

async fn run_bounds_eval(
    device: &Device,
    queue: &Queue,
    pipeline: &FilterBoundsEvalPipeline,
    resources: &FilterResources,
    input: &LayerBinding,
    selection: &LayerBinding,
    has_selection: &Buffer,
    in_bounds: IRect,
) -> IRect {
    let input_buf = write_bounds_buffer(device, queue, in_bounds);
    let output_buf = device.create_buffer(&BufferDescriptor {
        label: Some("filter bounds eval output"),
        size: 16,
        usage: BufferUsages::STORAGE | BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    });
    let mut ec = device.create_command_encoder(&Default::default());
    {
        let mut pass = ec.begin_compute_pass(&ComputePassDescriptor {
            label: Some("filter bounds eval pass"),
            ..Default::default()
        });
        pipeline.dispatch(
            device,
            &mut pass,
            input,
            selection,
            has_selection,
            &input_buf,
            &output_buf,
            resources,
        );
    }
    let readback = create_readback_buffer_and_schedule_copy_buffer(device, &mut ec, &output_buf);
    let readback_async = readback_buffer_raw_on_submit_async(&mut ec, &readback, ..);
    queue.submit([ec.finish()]);
    match readback_async.into_inner().await {
        Ok(Ok(bytes)) if bytes.len() >= 16 => {
            let vals: Vec<i32> = bytes[..16]
                .chunks_exact(4)
                .map(|c| i32::from_ne_bytes([c[0], c[1], c[2], c[3]]))
                .collect();
            let rect = IRect {
                min: IVec2::new(vals[0], vals[1]),
                max: IVec2::new(vals[2], vals[3]),
            };
            if rect.min.x < rect.max.x && rect.min.y < rect.max.y {
                return rect;
            }
            log::warn!("invalid rect {:?}; falling back", rect);
        }
        other => {
            log::warn!(
                "Filter bounds eval readback failed; falling back to input bounds ({:?})",
                other.is_err()
            );
        }
    }
    in_bounds
}
