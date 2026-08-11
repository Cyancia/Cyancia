use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use anyhow::Result;
use bevy_math::IRect;
use chrono::{DateTime, Utc};
use cyancia_assets::{AssetAppExt, store::AssetRegistry};
use cyancia_canvas::{CanvasAppExt, CanvasId};
use cyancia_color::ForegroundBackgroundColorExt;
use cyancia_image::{
    composite::{LayerPreviewOverriders, PixelPreviewOverrider},
    layer::{LayerId, properties::LayerTexelTypeProp},
    scan_pixels::ScanPixelsPipeline,
    texel::TexelType,
    tile::{DynamicLayerStorage, GpuLayerInfo, GpuTileStorage, LayerBinding, TileStorageAppExt},
};
use cyancia_input::mouse::PressedMouseState;
use cyancia_render::{
    buffer::{BufferVec, DynamicBuffer},
    readback::{
        AsyncBufferReadback, create_readback_buffer_and_schedule_copy,
        readback_buffer_on_submit_async,
    },
    render_context::RenderContextAppExt,
    texture::GpuImage,
    texture_atlas::{TextureAtlas, TextureAtlasBuilder},
};
use cyancia_runtime::Services;
use cyancia_shader_graph::graph::external::GraphExternalVariableStorage;
use cyancia_utils::log_err::LogErr;
use encase::ShaderType;
use futures::channel::oneshot;
use glam::{IVec2, Vec2, Vec4};
use iced_runtime::Task;
use parking_lot::Mutex;
use wgpu::{
    BindGroupEntry, BindGroupLayoutEntry, BindingResource, BindingType, Buffer, BufferBindingType,
    BufferDescriptor, BufferUsages, ComputePassDescriptor, Device, Extent3d, Queue, ShaderStages,
    TextureDescriptor, TextureDimension, TextureFormat, TextureUsages,
    util::{BufferInitDescriptor, DeviceExt},
};

use crate::{
    input_processing::{InputProcessor, RawPenInput},
    instance::{BrushPresetInstance, CompiledBrushPreset},
    render::{
        graph::CanvasResources,
        pipeline::{
            BrushInputSamplingPipeline, BrushMainBoundsEvalPipeline, BrushMainPipeline,
            BrushPostProcessBoundsEvalPipeline, BrushPostProcessPipeline,
        },
    },
};

pub mod graph;
pub mod pipeline;

const EXTERNAL_VARIABLE_BASE_BINDING: u32 = 32;
pub const MAX_DABS_PER_STROKE: u32 = 256;

pub struct CanvasBrushStrokeSessionInfo {
    pub stroke_id: u64,
    pub stroke_begin: DateTime<Utc>,
    pub canvas_id: CanvasId,
    pub target_layer_id: LayerId,
    pub selection_layer_id: LayerId,
    pub target_layer_format: TexelType,
    pub selection_layer_format: TexelType,
}

pub struct CanvasBrushPresetOperator {
    instance: BrushPresetInstance,
    device: Device,
    queue: Queue,
    renderer: Option<BrushPresetRenderer>,
    last_session: Option<CanvasBrushStrokeSessionInfo>,
    input_processor: InputProcessor,
    cached_brush: Option<CompiledBrushPreset>,
    canvas_resources: DynamicBuffer<CanvasResources>,
}

impl CanvasBrushPresetOperator {
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
            canvas_resources: DynamicBuffer::new(
                Some("canvas_resources".into()),
                BufferUsages::STORAGE,
            ),
        }
    }

    pub fn instance(&self) -> &BrushPresetInstance {
        &self.instance
    }

    pub fn instance_mut(&mut self) -> &mut BrushPresetInstance {
        self.cached_brush = None;
        &mut self.instance
    }

    pub fn begin_stroke(
        &mut self,
        input: &PressedMouseState,
        stroke_id: u64,
        canvas_id: CanvasId,
        services: &mut Services,
    ) -> Task<BrushRenderUpdate> {
        let canvas = services
            .canvas(&canvas_id)
            .expect("Current canvas should exist");
        let position = canvas
            .transform
            .window_to_pixel(Vec2::new(input.position.x, input.position.y));
        let active_layer_id = canvas.active_layer_id();
        let selection_layer_id = canvas.image.selection_layer();
        if !canvas
            .active_layer_node()
            .properties()
            .contains::<LayerTexelTypeProp>()
        {
            log::warn!("Unable to paint to the active layer which cannot contain pixels.");
            return Task::none();
        }

        // update canvas resources
        let xyz_to_rgb = canvas
            .image
            .profile()
            .rgb_to_xyz_matrix()
            .to_f32()
            .inverse();
        let fg_color = services.foreground_color().get().into_rgb(xyz_to_rgb);
        let bg_color = services.background_color().get().into_rgb(xyz_to_rgb);
        self.canvas_resources.clear();
        self.canvas_resources.push(&CanvasResources {
            foreground_color: Vec4::new(fg_color.r, fg_color.g, fg_color.b, 1.0),
            background_color: Vec4::new(bg_color.r, bg_color.g, bg_color.b, 1.0),
        });
        self.canvas_resources
            .write_buffer(&self.device, &self.queue);

        let tiles = services.tile_storage();
        let target_layer_info = tiles
            .get_layer_info(active_layer_id)
            .expect("Active pixel layer should have GPU storage");
        let selection_layer_info = tiles
            .get_layer_info(selection_layer_id)
            .expect("Selection layer should have GPU storage");
        let session = CanvasBrushStrokeSessionInfo {
            stroke_id,
            stroke_begin: Utc::now(),
            canvas_id,
            target_layer_id: active_layer_id,
            selection_layer_id,
            target_layer_format: target_layer_info.texel_type,
            selection_layer_format: selection_layer_info.texel_type,
        };
        if let Some(last_session) = self.last_session.as_ref()
            && (last_session.target_layer_format != session.target_layer_format
                || last_session.selection_layer_format != session.selection_layer_format)
        {
            self.renderer = None;
        }

        let compiled_brush = self.cached_brush.get_or_insert_with(|| {
            self.instance
                .compile(EXTERNAL_VARIABLE_BASE_BINDING)
                .expect("Failed to compile brush preset")
        });
        println!("Compiled brush:\n{}", compiled_brush);
        let renderer = self.renderer.get_or_insert_with(|| {
            BrushPresetRenderer::new(
                compiled_brush,
                session.target_layer_format,
                session.selection_layer_format,
                services,
                &self.canvas_resources,
            )
        });

        self.input_processor.reset();
        let tiles = services.tile_storage();
        let target_layer = tiles
            .get_layer_binding_or_empty(session.target_layer_id)
            .expect("Failed to bind active pixel layer");
        let selection_layer = tiles
            .get_layer_binding_or_empty(session.selection_layer_id)
            .expect("Failed to bind selection layer");
        renderer.begin(&self.device, &self.queue, target_layer, selection_layer);

        let sample = self
            .input_processor
            .push(RawPenInput::new(position, session.stroke_begin));
        let task = sample.map({
            |sample| {
                renderer
                    .update(&self.device, &self.queue, sample)
                    .map({
                        let target_layer_id = session.target_layer_id;

                        move |result| {
                            Some(BrushRenderUpdate::Preview {
                                stroke_id,
                                canvas_id,
                                target_layer_id,
                                overrider: PixelPreviewOverrider {
                                    texture: result.texture_view()?.clone(),
                                    tile_info_buffer: result.tile_info_buffer()?.clone(),
                                },
                                dirty_tiles: result.compute_tile_bounds(),
                            })
                        }
                    })
                    .then(|msg| msg.map(Task::done).unwrap_or(Task::none()))
            }
        });

        self.last_session = Some(session);
        task.unwrap_or_else(Task::none)
    }

    pub fn update_stroke(
        &mut self,
        input: &PressedMouseState,
        services: &Services,
    ) -> Task<BrushRenderUpdate> {
        let Some(renderer) = &mut self.renderer else {
            return Task::none();
        };
        let Some(session) = self.last_session.as_ref() else {
            return Task::none();
        };
        let canvas = services.canvas(&session.canvas_id).unwrap();
        let position = canvas
            .transform
            .window_to_pixel(Vec2::new(input.position.x, input.position.y));

        let Some(sample) = self
            .input_processor
            .push(RawPenInput::new(position, session.stroke_begin))
        else {
            return Task::none();
        };

        renderer
            .update(&self.device, &self.queue, sample)
            .map({
                let canvas_id = session.canvas_id;
                let target_layer_id = session.target_layer_id;
                let stroke_id = session.stroke_id;

                move |result| {
                    Some(BrushRenderUpdate::Preview {
                        stroke_id,
                        canvas_id,
                        target_layer_id,
                        overrider: PixelPreviewOverrider {
                            texture: result.texture_view()?.clone(),
                            tile_info_buffer: result.tile_info_buffer()?.clone(),
                        },
                        dirty_tiles: result.compute_tile_bounds(),
                    })
                }
            })
            .then(|msg| msg.map(Task::done).unwrap_or(Task::none()))
    }

    pub fn end_stroke(
        &mut self,
        input: &PressedMouseState,
        services: &mut Services,
    ) -> Task<BrushRenderUpdate> {
        let Some(renderer) = self.renderer.as_mut() else {
            return Task::none();
        };
        let Some(session) = self.last_session.take() else {
            return Task::none();
        };
        let canvas = services
            .canvas(&session.canvas_id)
            .expect("Stroke canvas should exist");
        let position = canvas
            .transform
            .window_to_pixel(Vec2::new(input.position.x, input.position.y));

        let mut updates = Vec::new();
        for sample in self
            .input_processor
            .flush(RawPenInput::new(position, session.stroke_begin))
        {
            updates.push(renderer.update(&self.device, &self.queue, sample));
        }
        let end_task = renderer.end(&self.device, &self.queue);
        services
            .service_mut::<LayerPreviewOverriders>()
            .remove_overrider(&session.target_layer_id);

        let updates = Task::batch(updates)
            .map({
                let canvas_id = session.canvas_id;
                let target_layer_id = session.target_layer_id;
                let stroke_id = session.stroke_id;

                move |result| {
                    Some(BrushRenderUpdate::Preview {
                        stroke_id,
                        canvas_id,
                        target_layer_id,
                        overrider: PixelPreviewOverrider {
                            texture: result.texture_view()?.clone(),
                            tile_info_buffer: result.tile_info_buffer()?.clone(),
                        },
                        dirty_tiles: result.compute_tile_bounds(),
                    })
                }
            })
            .then(|msg| msg.map(Task::done).unwrap_or(Task::none()));

        let end = end_task.map({
            move |result| BrushRenderUpdate::Finished {
                stroke_id: session.stroke_id,
                canvas_id: session.canvas_id,
                target_layer_id: session.target_layer_id,
                result,
            }
        });

        updates.chain(end)
    }
}

#[derive(Clone)]
struct StrokePostprocessPipelines {
    main: BrushPostProcessPipeline,
    bounds_eval: BrushPostProcessBoundsEvalPipeline,
}

struct StrokeSession {
    shared: Arc<Mutex<SharedBrushRendererState>>,
    target_layer: LayerBinding,
    has_selection: Buffer,
    selection_layer: LayerBinding,
    previous_finished: oneshot::Receiver<()>,
}

pub enum BrushRenderUpdate {
    Preview {
        stroke_id: u64,
        canvas_id: CanvasId,
        target_layer_id: LayerId,
        overrider: PixelPreviewOverrider,
        dirty_tiles: IRect,
    },
    Finished {
        stroke_id: u64,
        canvas_id: CanvasId,
        target_layer_id: LayerId,
        result: DynamicLayerStorage,
    },
}

pub struct BrushPresetRenderer {
    input_sample: BrushInputSamplingPipeline,
    main: BrushMainPipeline,
    main_bounds_eval: BrushMainBoundsEvalPipeline,
    stroke_pp: Vec<StrokePostprocessPipelines>,
    resources: StrokeResources,
    scan_pixels: ScanPixelsPipeline,

    input_sampler_buffer: DynamicBuffer<InputSampler>,
    session: Option<StrokeSession>,
}

impl BrushPresetRenderer {
    #[tracing::instrument(skip_all, name = "new_renderer")]
    pub fn new(
        brush: &CompiledBrushPreset,
        target_layer_format: TexelType,
        selection_layer_format: TexelType,
        services: &Services,
        canvas_resources: &DynamicBuffer<CanvasResources>,
    ) -> Self {
        let device = services.render_device();
        let queue = services.render_queue();
        let assets = services.assets();

        let resources = StrokeResources::new(
            device,
            queue,
            brush,
            target_layer_format,
            selection_layer_format,
            assets,
            canvas_resources,
        );
        let scan_pixels = ScanPixelsPipeline::new(device, selection_layer_format);

        let input_sample = BrushInputSamplingPipeline::new(
            device,
            &resources,
            brush.input_sampling.clone().into(),
        );

        let main = BrushMainPipeline::new(device, &resources, brush.main_graph.main.clone().into());
        let main_bounds_eval = BrushMainBoundsEvalPipeline::new(
            device,
            &resources,
            brush.main_graph.bounds_eval.clone().into(),
        );

        let mut stroke_pp = Vec::new();
        for graph in &brush.stroke_postprocess_graphs {
            let main = BrushPostProcessPipeline::new(device, &resources, graph.main.clone().into());
            let bounds_eval = BrushPostProcessBoundsEvalPipeline::new(
                device,
                &resources,
                graph.bounds_eval.clone().into(),
            );
            stroke_pp.push(StrokePostprocessPipelines { main, bounds_eval });
        }

        let mut input_sampler_buffer =
            DynamicBuffer::new(Some("input sampler buffer".into()), BufferUsages::STORAGE);
        input_sampler_buffer.push(&InputSampler::default());
        input_sampler_buffer.write_buffer(device, queue);

        Self {
            input_sample,
            main,
            main_bounds_eval,
            stroke_pp,
            resources,
            scan_pixels,

            input_sampler_buffer,
            session: None,
        }
    }

    pub fn begin(
        &mut self,
        device: &Device,
        queue: &Queue,
        target_layer: LayerBinding,
        selection_layer: LayerBinding,
    ) {
        self.input_sampler_buffer.clear();
        self.input_sampler_buffer.push(&InputSampler::default());
        self.input_sampler_buffer.write_buffer(device, queue);

        let has_selection = self
            .scan_pixels
            .scan_to_binary_buffer(device, queue, &selection_layer);

        let shared = SharedBrushRendererState {
            intermediate_buffers: [
                DynamicLayerStorage::new(
                    device.clone(),
                    queue.clone(),
                    GpuLayerInfo {
                        texel_type: self.resources.target_layer_format,
                    },
                ),
                DynamicLayerStorage::new(
                    device.clone(),
                    queue.clone(),
                    GpuLayerInfo {
                        texel_type: self.resources.target_layer_format,
                    },
                ),
            ],
            round: 0,
            accumulated_tile_bounds: IRect::EMPTY,
        };
        let (previous_finished_tx, previous_finished_rx) = oneshot::channel();
        previous_finished_tx.send(()).ok();

        self.session = Some(StrokeSession {
            shared: Arc::new(Mutex::new(shared)),
            target_layer,
            has_selection,
            selection_layer,
            previous_finished: previous_finished_rx,
        });
    }

    pub fn end(&mut self, device: &Device, queue: &Queue) -> Task<DynamicLayerStorage> {
        let Some(session) = self.session.take() else {
            return Task::none();
        };

        let device = device.clone();
        let queue = queue.clone();
        let stroke_pp = self.stroke_pp.clone();
        let resources = self.resources.clone();
        let scan_pixels = self.scan_pixels.clone();

        Task::future(async move {
            brush_renderer_worker_stroke_postprocess(
                session.shared,
                device,
                queue,
                stroke_pp,
                resources,
                scan_pixels,
                session.target_layer,
                session.selection_layer,
            )
            .await
        })
    }

    // TODO: Copy unchanged tiles onto another buffer?
    pub fn update(
        &mut self,
        device: &Device,
        queue: &Queue,
        pen_input: PenInput,
    ) -> Task<DynamicLayerStorage> {
        let Some(session) = &mut self.session else {
            return Task::none();
        };

        self.resources.update_external_var_buffers(queue);

        let mut pen_input_buffer =
            DynamicBuffer::new(Some("pen input buffer".into()), BufferUsages::STORAGE);
        pen_input_buffer.push(&pen_input);
        pen_input_buffer.write_buffer(device, queue);

        let bounds_eval_dispatch = device.create_buffer(&BufferDescriptor {
            label: Some("bounds eval dispatch"),
            size: 16,
            usage: BufferUsages::STORAGE | BufferUsages::INDIRECT,
            mapped_at_creation: false,
        });
        let mut output_samples = DynamicBuffer::new(
            Some("output samples buffer".into()),
            BufferUsages::COPY_SRC | BufferUsages::STORAGE,
        );
        // TODO Use uninit buffer
        output_samples.push(&OutputSamples::new(MAX_DABS_PER_STROKE));
        output_samples.write_buffer(device, queue);

        let mut dab_infos = BufferVec::new(
            Some("dab info buffer".into()),
            BufferUsages::COPY_SRC | BufferUsages::STORAGE,
        );
        // TODO Use uninit buffer
        for _ in 0..MAX_DABS_PER_STROKE {
            dab_infos.push(&DabInfo::default());
        }
        dab_infos.write_buffer(device, queue);

        let mut ec = device.create_command_encoder(&Default::default());

        ec.push_debug_group("brush preset update stroke");
        {
            let mut pass = ec.begin_compute_pass(&ComputePassDescriptor {
                label: Some("brush preset update pass"),
                ..Default::default()
            });

            self.input_sample.dispatch(
                device,
                &mut pass,
                &pen_input_buffer,
                &self.input_sampler_buffer,
                &output_samples,
                &bounds_eval_dispatch,
                &self.resources,
            );
            self.main_bounds_eval.dispatch(
                device,
                &mut pass,
                &output_samples,
                &dab_infos,
                &session.target_layer.texture,
                &session.target_layer.tile_info_buffer,
                &session.has_selection,
                &session.selection_layer.texture,
                &session.selection_layer.tile_info_buffer,
                &self.resources,
            );
        }
        ec.pop_debug_group();

        let output_samples_readback = create_readback_buffer_and_schedule_copy(
            device,
            &mut ec,
            output_samples.inner_buffer().unwrap(),
        );
        let dab_info_readback = create_readback_buffer_and_schedule_copy(
            device,
            &mut ec,
            dab_infos.inner_buffer().unwrap(),
        );
        let samples_readback =
            readback_buffer_on_submit_async(&mut ec, &output_samples_readback, ..);
        let dab_info_readback = readback_buffer_on_submit_async(&mut ec, &dab_info_readback, ..);

        // unsafe {
        //     device.start_graphics_debugger_capture();
        // }
        queue.submit([ec.finish()]);
        // unsafe {
        //     device.stop_graphics_debugger_capture();
        // }

        let shared = session.shared.clone();
        let device = device.clone();
        let queue = queue.clone();
        let main = self.main.clone();
        let stroke_pp = self.stroke_pp.clone();
        let resources = self.resources.clone();
        let scan_pixels = self.scan_pixels.clone();
        let target_layer = session.target_layer.clone();
        let selection_layer = session.selection_layer.clone();

        let (new_previous_finished_tx, new_previous_finished_rx) = oneshot::channel();
        let previous_finished =
            std::mem::replace(&mut session.previous_finished, new_previous_finished_rx);

        Task::future(async move {
            brush_renderer_worker_main(
                previous_finished,
                new_previous_finished_tx,
                shared,
                samples_readback,
                dab_info_readback,
                device,
                queue,
                main,
                stroke_pp,
                resources,
                scan_pixels,
                target_layer,
                selection_layer,
            )
            .await
        })
        .then(|result| {
            if let Ok(result) = result.logged_err() {
                Task::done(result)
            } else {
                Task::none()
            }
        })
    }
}

struct SharedBrushRendererState {
    intermediate_buffers: [DynamicLayerStorage; 2],
    round: u32,
    accumulated_tile_bounds: IRect,
}

async fn brush_renderer_worker_main(
    previous_finished: oneshot::Receiver<()>,
    next_start: oneshot::Sender<()>,
    shared: Arc<Mutex<SharedBrushRendererState>>,
    samples: AsyncBufferReadback<OutputSamples>,
    dab_infos: AsyncBufferReadback<Vec<DabInfo>>,

    device: Device,
    queue: Queue,

    main: BrushMainPipeline,
    stroke_pp: Vec<StrokePostprocessPipelines>,
    resources: StrokeResources,
    scan_pixels: ScanPixelsPipeline,

    target_layer: LayerBinding,
    selection_layer: LayerBinding,
) -> Result<DynamicLayerStorage> {
    let _ = previous_finished.await;

    let preview_cancelled = Arc::new(AtomicBool::new(false));

    let samples = samples.into_inner().await??;
    let dab_infos = dab_infos.into_inner().await??;

    let (mut intermediate_buffers, mut accumulated_tile_bounds, mut round) = {
        let mut shared = shared.lock();
        let has_selection = scan_pixels.scan_to_binary_buffer(&device, &queue, &selection_layer);

        let dispatch_span = tracing::info_span!("main_dispatch");
        let _span = dispatch_span.enter();

        let mut samples_buffer =
            DynamicBuffer::new(Some("output samples buffer".into()), BufferUsages::STORAGE);
        let mut samples_offsets = Vec::new();
        let mut dab_infos_buffer = DynamicBuffer::new(
            Some("output dab infos buffer".into()),
            BufferUsages::STORAGE,
        );
        let mut dab_info_offsets = Vec::new();

        for (sample, dab_info) in samples
            .samples
            .into_iter()
            .take(samples.n_samples as usize)
            .zip(dab_infos)
        {
            samples_offsets.push(samples_buffer.push(&sample) as u32);
            dab_info_offsets.push(dab_infos_buffer.push(&dab_info) as u32);

            for b in &mut shared.intermediate_buffers {
                b.allocate_tiles(IRect {
                    min: dab_info.bound_min,
                    max: dab_info.bound_max,
                });
            }
            shared.accumulated_tile_bounds = shared.accumulated_tile_bounds.union(IRect {
                min: dab_info.bound_min,
                max: dab_info.bound_max,
            });
        }

        if shared.accumulated_tile_bounds.is_empty() {
            return Err(anyhow::anyhow!("accumulated_tile_bounds is empty"));
        }

        samples_buffer.write_buffer(&device, &queue);
        dab_infos_buffer.write_buffer(&device, &queue);

        let mut ec = device.create_command_encoder(&Default::default());

        {
            let mut pass = ec.begin_compute_pass(&ComputePassDescriptor {
                label: Some("brush main pass"),
                ..Default::default()
            });
            main.dispatch(
                &device,
                &mut pass,
                &target_layer.texture,
                &target_layer.tile_info_buffer,
                &has_selection,
                &selection_layer.texture,
                &selection_layer.tile_info_buffer,
                &samples_buffer,
                &samples_offsets,
                &dab_infos_buffer,
                &dab_info_offsets,
                &resources,
                &[
                    shared.intermediate_buffers[0].binding().unwrap(),
                    shared.intermediate_buffers[1].binding().unwrap(),
                ],
                &mut shared.round,
            );
        }

        let intermediate_buffers = [
            shared.intermediate_buffers[0].deep_clone(),
            shared.intermediate_buffers[1].deep_clone(),
        ];
        let accumulated_tile_bounds = shared.accumulated_tile_bounds;
        let round = shared.round;

        queue.submit([ec.finish()]);

        (intermediate_buffers, accumulated_tile_bounds, round)
    };

    next_start.send(()).ok();

    if preview_cancelled.load(Ordering::Acquire) {
        return Err(anyhow::anyhow!("preview cancelled"));
    }
    // unsafe {
    //     device.start_graphics_debugger_capture();
    // }
    postprocess_stroke(
        &device,
        &queue,
        &target_layer,
        &selection_layer,
        Time::default(),
        &mut intermediate_buffers,
        &mut round,
        &mut accumulated_tile_bounds,
        &scan_pixels,
        &stroke_pp,
        &resources,
    )
    .await;
    // unsafe {
    //     device.stop_graphics_debugger_capture();
    // }

    if accumulated_tile_bounds.is_empty() {
        return Err(anyhow::anyhow!("accumulated_tile_bounds is empty"));
    }

    let [buffer_a, buffer_b] = intermediate_buffers;
    Ok(if round % 2 == 0 { buffer_a } else { buffer_b })
}

async fn brush_renderer_worker_stroke_postprocess(
    shared: Arc<Mutex<SharedBrushRendererState>>,

    device: Device,
    queue: Queue,

    stroke_pp: Vec<StrokePostprocessPipelines>,
    resources: StrokeResources,
    scan_pixels: ScanPixelsPipeline,

    target_layer: LayerBinding,
    selection_layer: LayerBinding,
) -> DynamicLayerStorage {
    let (mut intermediate_buffers, mut round, mut accumulated_tile_bounds) = {
        let shared = shared.lock();
        (
            [
                shared.intermediate_buffers[0].deep_clone(),
                shared.intermediate_buffers[1].deep_clone(),
            ],
            shared.round,
            shared.accumulated_tile_bounds,
        )
    };

    postprocess_stroke(
        &device,
        &queue,
        &target_layer,
        &selection_layer,
        // TODO
        Time::default(),
        &mut intermediate_buffers,
        &mut round,
        &mut accumulated_tile_bounds,
        &scan_pixels,
        &stroke_pp,
        &resources,
    )
    .await;

    let [buffer_a, buffer_b] = intermediate_buffers;
    if round % 2 == 0 { buffer_a } else { buffer_b }
}

async fn postprocess_stroke(
    device: &Device,
    queue: &Queue,
    target_layer: &LayerBinding,
    selection_layer: &LayerBinding,
    time: Time,
    intermediate_buffers: &mut [DynamicLayerStorage; 2],
    round: &mut u32,
    accumulated_tile_bounds: &mut IRect,
    scan_pixels: &ScanPixelsPipeline,
    stroke_pp: &[StrokePostprocessPipelines],
    resources: &StrokeResources,
) {
    if accumulated_tile_bounds.is_empty() {
        return;
    }

    let has_selection = scan_pixels.scan_to_binary_buffer(device, queue, selection_layer);

    let mut dab_info_buffer = DynamicBuffer::new(
        Some("pp dab info buffer".into()),
        BufferUsages::STORAGE | BufferUsages::COPY_SRC,
    );
    let mut stroke_pp_data =
        DynamicBuffer::new(Some("stroke pp data".into()), BufferUsages::STORAGE);

    for pipeline in stroke_pp {
        dab_info_buffer.clear();
        dab_info_buffer.push(&DabInfo::default());
        dab_info_buffer.write_buffer(device, queue);

        stroke_pp_data.clear();
        stroke_pp_data.push(&StrokePostprocessData {
            accumulated_pixel_bounds: GpuTileStorage::tile_rect_to_pixel(*accumulated_tile_bounds),
            time,
        });
        stroke_pp_data.write_buffer(device, queue);

        let mut ec = device.create_command_encoder(&Default::default());
        ec.push_debug_group("brush preset stroke postprocess");

        {
            let mut pass = ec.begin_compute_pass(&ComputePassDescriptor {
                label: Some("brush preset stroke postprocess pass"),
                ..Default::default()
            });

            pipeline.bounds_eval.dispatch(
                device,
                &mut pass,
                &stroke_pp_data,
                &target_layer.texture,
                &target_layer.tile_info_buffer,
                &has_selection,
                &selection_layer.texture,
                &selection_layer.tile_info_buffer,
                &dab_info_buffer,
                resources,
            );
        }

        let dab_info_readback_buffer = create_readback_buffer_and_schedule_copy(
            device,
            &mut ec,
            dab_info_buffer.inner_buffer().unwrap(),
        );
        let dab_info_readback =
            readback_buffer_on_submit_async::<DabInfo, _>(&mut ec, &dab_info_readback_buffer, ..);

        ec.pop_debug_group();
        // unsafe {
        //     device.start_graphics_debugger_capture();
        // }
        queue.submit([ec.finish()]);
        // unsafe {
        //     device.stop_graphics_debugger_capture();
        // }

        let new_dab_info = dab_info_readback.into_inner().await.unwrap().unwrap();
        *accumulated_tile_bounds = IRect {
            min: new_dab_info.bound_min,
            max: new_dab_info.bound_max,
        };

        let mut dab_info = DynamicBuffer::new(Some("pp dab info".into()), BufferUsages::STORAGE);
        dab_info.push(&new_dab_info);
        dab_info.write_buffer(device, queue);

        for b in intermediate_buffers.iter_mut() {
            b.allocate_tiles(IRect {
                min: new_dab_info.bound_min,
                max: new_dab_info.bound_max,
            });
        }

        let intermediate_buffers = [
            intermediate_buffers[0].binding().unwrap(),
            intermediate_buffers[1].binding().unwrap(),
        ];

        let mut ec = device.create_command_encoder(&Default::default());

        {
            let mut pass = ec.begin_compute_pass(&Default::default());
            pipeline.main.dispatch(
                device,
                &mut pass,
                &stroke_pp_data,
                &target_layer.texture,
                &target_layer.tile_info_buffer,
                &has_selection,
                &selection_layer.texture,
                &selection_layer.tile_info_buffer,
                &dab_info,
                resources,
                &intermediate_buffers,
                round,
            );
        }
        queue.submit([ec.finish()]);
    }
}

#[derive(ShaderType, Debug, Clone)]
pub struct OutputSamples {
    pub n_samples: u32,
    pub is_overflow: u32,
    #[shader(size(runtime))]
    pub samples: Vec<ComputedPenInput>,
}

impl OutputSamples {
    pub fn new(max_samples: u32) -> Self {
        Self {
            n_samples: 0,
            is_overflow: 0,
            samples: vec![ComputedPenInput::default(); max_samples as usize],
        }
    }
}

#[derive(ShaderType, Debug, Default, Clone, Copy)]
pub struct InputSampler {
    pub last_input: PenInput,
    pub last_sample: ComputedPenInput,
    pub has_last_sample: u32,
}

#[derive(ShaderType, Debug, Default, Clone, Copy)]
pub struct PenInput {
    pub position: Vec2,
    pub tilt: Vec2,
    pub angle: Vec2,
    pub pressure: f32,
    pub time: Time,
    pub bezier_control_prev: Vec2,
    pub bezier_control_next: Vec2,
}

#[derive(ShaderType, Debug, Default, Clone, Copy)]
pub struct ComputedPenInput {
    pub position: Vec2,
    pub draw_direction_vec: Vec2,
    pub tilt: Vec2,
    pub angle: Vec2,
    pub draw_direction_angle: f32,
    pub pressure: f32,
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

#[derive(Clone)]
// TODO This should be renamed to RendererResources
pub struct StrokeResources {
    pub external_var_storage: Arc<GraphExternalVariableStorage>,
    pub external_var_layouts: Vec<BindGroupLayoutEntry>,
    pub external_var_buffers: Vec<Buffer>,
    pub referenced_textures: TextureAtlas,
    pub canvas_resources: Buffer,

    pub target_layer_format: TexelType,
    pub selection_layer_format: TexelType,
}

impl StrokeResources {
    fn new(
        device: &Device,
        queue: &Queue,
        brush: &CompiledBrushPreset,
        target_layer_format: TexelType,
        selection_layer_format: TexelType,
        assets: &AssetRegistry,
        canvas_resources: &DynamicBuffer<CanvasResources>,
    ) -> Self {
        let mut external_var_layouts = Vec::new();
        for cur_binding in (EXTERNAL_VARIABLE_BASE_BINDING..).take(brush.external_vars.all().len())
        {
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
        }

        let mut external_var_buffers = Vec::new();
        for var in brush.external_vars.all().iter() {
            let buffer = var.value.try_write_into_shader_buffer().unwrap();
            let gpu_buffer = device.create_buffer_init(&BufferInitDescriptor {
                label: Some("external variable buffer"),
                contents: &buffer,
                usage: BufferUsages::STORAGE | BufferUsages::COPY_DST,
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
            if let Some(asset_id) = **id {
                let handle = assets.handle(asset_id).unwrap();
                let gpu_image = GpuImage::from_asset(
                    device,
                    queue,
                    &handle.get().unwrap(),
                    // TODO: This is weird but, adding TEXTURE_BINDING usage to avoid vulkan validation error:
                    // VALIDATION [VUID-VkImageViewCreateInfo-image-04441 (0xb75da543)]
                    // vkCreateImageView(): pCreateInfo->image (VkImage 0xb550000000b55) was created with VK_IMAGE_USAGE_TRANSFER_SRC_BIT|VK_IMAGE_USAGE_TRANSFER_DST_BIT but requires VK_IMAGE_USAGE_SAMPLED_BIT|VK_IMAGE_USAGE_STORAGE_BIT|VK_IMAGE_USAGE_COLOR_ATTACHMENT_BIT|VK_IMAGE_USAGE_DEPTH_STENCIL_ATTACHMENT_BIT|VK_IMAGE_USAGE_TRANSIENT_ATTACHMENT_BIT|VK_IMAGE_USAGE_INPUT_ATTACHMENT_BIT|VK_IMAGE_USAGE_FRAGMENT_SHADING_RATE_ATTACHMENT_BIT_KHR|VK_IMAGE_USAGE_FRAGMENT_DENSITY_MAP_BIT_EXT|VK_IMAGE_USAGE_VIDEO_DECODE_DST_BIT_KHR|VK_IMAGE_USAGE_VIDEO_DECODE_DPB_BIT_KHR|VK_IMAGE_USAGE_VIDEO_ENCODE_SRC_BIT_KHR|VK_IMAGE_USAGE_VIDEO_ENCODE_DPB_BIT_KHR|VK_IMAGE_USAGE_SAMPLE_WEIGHT_BIT_QCOM|VK_IMAGE_USAGE_SAMPLE_BLOCK_MATCH_BIT_QCOM|VK_IMAGE_USAGE_VIDEO_ENCODE_QUANTIZATION_DELTA_MAP_BIT_KHR|VK_IMAGE_USAGE_VIDEO_ENCODE_EMPHASIS_MAP_BIT_KHR.
                    // The Vulkan spec states: image must have been created with a usage value containing at least one of the following: VK_IMAGE_USAGE_SAMPLED_BIT VK_IMAGE_USAGE_STORAGE_BIT VK_IMAGE_USAGE_COLOR_ATTACHMENT_BIT VK_IMAGE_USAGE_DEPTH_STENCIL_ATTACHMENT_BIT VK_IMAGE_USAGE_INPUT_ATTACHMENT_BIT VK_IMAGE_USAGE_TRANSIENT_ATTACHMENT_BIT VK_IMAGE_USAGE_FRAGMENT_SHADING_RATE_ATTACHMENT_BIT_KHR VK_IMAGE_USAGE_FRAGMENT_DENSITY_MAP_BIT_EXT VK_IMAGE_USAGE_VIDEO_DECODE_DST_BIT_KHR VK_IMAGE_USAGE_VIDEO_DECODE_DPB_BIT_KHR VK_IMAGE_USAGE_VIDEO_ENCODE_SRC_BIT_KHR VK_IMAGE_USAGE_VIDEO_ENCODE_DPB_BIT_KHR VK_IMAGE_USAGE_SAMPLE_WEIGHT_BIT_QCOM VK_IMAGE_USAGE_SAMPLE_BLOCK_MATCH_BIT_QCOM VK_IMAGE_USAGE_VIDEO_ENCODE_QUANTIZATION_DELTA_MAP_BIT_KHR VK_IMAGE_USAGE_VIDEO_ENCODE_EMPHASIS_MAP_BIT_KHR (https://docs.vulkan.org/spec/latest/chapters/resources.html#VUID-VkImageViewCreateInfo-image-04441)
                    TextureUsages::COPY_SRC | TextureUsages::TEXTURE_BINDING,
                );
                referenced_textures_builder.add_texture(gpu_image.texture.clone());
            } else {
                referenced_textures_builder.add_texture(empty_texture.clone());
            }
        }
        if referenced_textures_builder.is_empty() {
            referenced_textures_builder.add_texture(empty_texture.clone());
        }
        let referenced_textures = referenced_textures_builder
            .build(Some("referenced textures"), device, queue)
            .unwrap();

        Self {
            external_var_storage: brush.external_vars.clone(),
            external_var_layouts,
            external_var_buffers,
            referenced_textures,

            target_layer_format,
            selection_layer_format,
            canvas_resources: canvas_resources.inner_buffer().unwrap().clone(),
        }
    }

    pub fn update_external_var_buffers(&mut self, queue: &Queue) {
        for (ext_var, var_buffer) in self
            .external_var_storage
            .all()
            .iter()
            .zip(&self.external_var_buffers)
        {
            if let Some(buffer) = ext_var.value.try_write_into_shader_buffer() {
                queue.write_buffer(var_buffer, 0, &buffer);
            }
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
