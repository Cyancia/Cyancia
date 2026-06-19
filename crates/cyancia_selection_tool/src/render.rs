use std::{borrow::Cow, sync::OnceLock, time::Instant};

use bevy_math::IRect;
use cyancia_canvas::{CanvasAppExt, command::TileReplaceCommand, control::CanvasTransform};
use cyancia_image::{
    texel::TexelType,
    tile::{
        DynamicLayerStorage, GpuLayerInfo, GpuTileInfo, GpuTileStorage, GpuTileStorageInner,
        LayerBindingData,
    },
};
use cyancia_render::{buffer::DynamicBuffer, render_context::RenderContext};
use encase::ShaderType;
use glam::{IVec2, Mat3, Vec2};
use gpui::{App, FillOptions, FillRule, Modifiers};
use indexmap::IndexSet;
use lyon::{
    path::Path,
    tessellation::{BuffersBuilder, FillTessellator, FillVertex, VertexBuffers},
};
use wesl::include_wesl;
use wgpu::{
    BindGroupDescriptor, BindGroupEntry, BindGroupLayout, BindGroupLayoutDescriptor,
    BindGroupLayoutEntry, BindingResource, BindingType, BufferBindingType, BufferUsages,
    ColorTargetState, ColorWrites, ComputePassDescriptor, ComputePipeline,
    ComputePipelineDescriptor, Device, FragmentState, IndexFormat, LoadOp, Operations,
    PipelineLayoutDescriptor, Queue, RenderPassColorAttachment, RenderPassDescriptor,
    RenderPipeline, RenderPipelineDescriptor, ShaderModuleDescriptor, ShaderSource, ShaderStages,
    StorageTextureAccess, StoreOp, TextureFormat, TextureView, TextureViewDimension,
    VertexAttribute, VertexBufferLayout, VertexFormat, VertexState, VertexStepMode,
    util::{BufferInitDescriptor, DeviceExt},
};

pub fn generate_cmd(
    label: Cow<'static, str>,
    vertices: &[Vec2],
    indices: &[u32],
    aabb_ps: IRect,
    op: SelectionOperation,
    cx: &mut App,
) -> Option<TileReplaceCommand> {
    let canvas = cx.read_current_canvas()?;
    let canvas_id = canvas.id();

    let tiles = cx.global::<GpuTileStorage>();
    let render_context = cx.global::<RenderContext>();
    let selection_layer_id = canvas.image.selection_layer();

    let affected_tiles = GpuTileStorageInner::pixel_rect_to_tile(aabb_ps);

    let selection_layer = tiles.get_layer(selection_layer_id).unwrap();
    let selection_layer_format = selection_layer.layer_info().texel_type;
    let selection_layer_binding = selection_layer
        .binding_data()
        .unwrap_or_else(|| tiles.empty_layer_binding(selection_layer_format));

    let mut pipeline = SelectionPipeline::new(&render_context.device, selection_layer_format);
    unsafe {
        render_context.device.start_graphics_debugger_capture();
    };
    let selection = pipeline.draw(
        &render_context.device,
        &render_context.queue,
        affected_tiles,
        vertices,
        indices,
        op,
        selection_layer_binding,
        selection_layer.iter_tiles().map(|(i, _, _)| i).collect(),
    )?;
    unsafe {
        render_context.device.stop_graphics_debugger_capture();
    };

    Some(TileReplaceCommand::new(
        label,
        canvas_id,
        &render_context.device,
        &render_context.queue,
        selection_layer_id,
        &selection_layer,
        selection.iter_tiles().map(|(i, _, _)| i).collect(),
        selection.texture().unwrap().texture().clone(),
    ))
}

#[derive(Debug, Clone, Copy)]
#[repr(u32)]
pub enum SelectionOperation {
    Replace,
    Intersect,
    Union,
    Subtract,
    SymmetricDifference,
}

impl SelectionOperation {
    pub fn from_modifiers(modifiers: Modifiers) -> Self {
        if modifiers == MODIFIERS_INTERSECT {
            return Self::Intersect;
        }
        if modifiers == MODIFIERS_UNION {
            return Self::Union;
        }
        if modifiers == MODIFIERS_SUBTRACT {
            return Self::Subtract;
        }
        if modifiers == MODIFIERS_SYMMETRIC_DIFFERENCE {
            return Self::SymmetricDifference;
        }
        Self::Replace
    }
}

pub const MODIFIERS_INTERSECT: Modifiers = Modifiers {
    control: false,
    alt: true,
    shift: true,
    platform: false,
    function: false,
};

pub const MODIFIERS_UNION: Modifiers = Modifiers {
    control: false,
    alt: false,
    shift: true,
    platform: false,
    function: false,
};

pub const MODIFIERS_SUBTRACT: Modifiers = Modifiers {
    control: false,
    alt: true,
    shift: false,
    platform: false,
    function: false,
};

pub const MODIFIERS_SYMMETRIC_DIFFERENCE: Modifiers = Modifiers {
    control: true,
    alt: true,
    shift: false,
    platform: false,
    function: false,
};

#[derive(ShaderType, Debug, Clone, Copy)]
struct SelectionParams {
    operation_ty: u32,
}

pub struct SelectionPipeline {
    render_layout: BindGroupLayout,
    render_pipeline: RenderPipeline,
    composite_layout: BindGroupLayout,
    composite_pipeline: ComputePipeline,
    layer_format: TexelType,
}

impl SelectionPipeline {
    pub fn new(device: &Device, layer_format: TexelType) -> Self {
        let render_layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some("selection_render_layout"),
            entries: &[BindGroupLayoutEntry {
                binding: 0,
                visibility: ShaderStages::VERTEX,
                ty: BindingType::Buffer {
                    ty: BufferBindingType::Uniform,
                    has_dynamic_offset: true,
                    min_binding_size: Some(IVec2::min_size()),
                },
                count: None,
            }],
        });

        let render_pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: Some("selection_render_pipeline_layout"),
            bind_group_layouts: &[Some(&render_layout)],
            immediate_size: 0,
        });

        let render_shader = device.create_shader_module(ShaderModuleDescriptor {
            label: Some("selection_render_shader"),
            source: ShaderSource::Wgsl(include_wesl!("render").into()),
        });

        let render_pipeline = device.create_render_pipeline(&RenderPipelineDescriptor {
            label: Some("selection_render_pipeline"),
            layout: Some(&render_pipeline_layout),
            vertex: VertexState {
                module: &render_shader,
                entry_point: "vertex".into(),
                compilation_options: Default::default(),
                buffers: &[VertexBufferLayout {
                    array_stride: 8,
                    step_mode: VertexStepMode::Vertex,
                    attributes: &[
                        // Position in pixel space
                        VertexAttribute {
                            shader_location: 0,
                            format: VertexFormat::Float32x2,
                            offset: 0,
                        },
                    ],
                }],
            },
            fragment: Some(FragmentState {
                module: &render_shader,
                entry_point: "fragment".into(),
                compilation_options: Default::default(),
                targets: &[Some(ColorTargetState {
                    format: layer_format.wgpu_format(),
                    blend: None,
                    write_mask: ColorWrites::ALL,
                })],
            }),
            primitive: Default::default(),
            depth_stencil: Default::default(),
            multisample: Default::default(),
            multiview_mask: Default::default(),
            cache: Default::default(),
        });

        let composite_layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some("selection_composite_layout"),
            entries: &[
                BindGroupLayoutEntry {
                    binding: 0,
                    visibility: ShaderStages::COMPUTE,
                    ty: BindingType::StorageTexture {
                        access: StorageTextureAccess::ReadOnly,
                        format: layer_format.wgpu_format(),
                        view_dimension: TextureViewDimension::D2Array,
                    },
                    count: None,
                },
                BindGroupLayoutEntry {
                    binding: 1,
                    visibility: ShaderStages::COMPUTE,
                    ty: BindingType::Buffer {
                        ty: BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: Some(GpuTileInfo::min_size()),
                    },
                    count: None,
                },
                BindGroupLayoutEntry {
                    binding: 2,
                    visibility: ShaderStages::COMPUTE,
                    ty: BindingType::StorageTexture {
                        access: StorageTextureAccess::ReadWrite,
                        format: layer_format.wgpu_format(),
                        view_dimension: TextureViewDimension::D2Array,
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
                    ty: BindingType::Buffer {
                        ty: BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: Some(SelectionParams::min_size()),
                    },
                    count: None,
                },
            ],
        });

        let composite_shader = device.create_shader_module(ShaderModuleDescriptor {
            label: Some("selection_composite_shader"),
            source: ShaderSource::Wgsl(include_wesl!("composite").into()),
        });

        let composite_pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: Some("selection_composite_pipeline_layout"),
            bind_group_layouts: &[Some(&composite_layout)],
            immediate_size: 0,
        });

        let composite_pipeline = device.create_compute_pipeline(&ComputePipelineDescriptor {
            label: Some("selection_composite_pipeline"),
            layout: Some(&composite_pipeline_layout),
            module: &composite_shader,
            entry_point: Some("main"),
            compilation_options: Default::default(),
            cache: None,
        });

        Self {
            render_layout,
            render_pipeline,
            composite_layout,
            composite_pipeline,
            layer_format,
        }
    }

    #[tracing::instrument(skip_all)]
    pub fn draw(
        &mut self,
        device: &Device,
        queue: &Queue,
        tile_aabb: IRect,
        vertices: &[Vec2],
        // Must be in counter-clockwise order
        indices: &[u32],
        op: SelectionOperation,
        target_selection: LayerBindingData,
        target_selection_tile_indices: IndexSet<IVec2>,
    ) -> Option<DynamicLayerStorage> {
        let selection = self.render_with_target_selection_reserved_output(
            device,
            queue,
            tile_aabb,
            vertices,
            indices,
            target_selection_tile_indices,
        )?;
        self.composite_with_target_selection_reserved_input(
            device,
            queue,
            op,
            &selection,
            &target_selection,
        )
    }

    /// Render the selection mesh with only affected tiles.
    #[tracing::instrument(skip_all)]
    pub fn render_with_tight_output(
        &mut self,
        device: &Device,
        queue: &Queue,
        tile_aabb: IRect,
        vertices: &[Vec2],
        // Must be in counter-clockwise order
        indices: &[u32],
    ) -> Option<DynamicLayerStorage> {
        self.render_internal(device, queue, tile_aabb, vertices, indices, IndexSet::new())
    }

    /// Render the selection mesh with affected tiles and reserve tiles that exist in target selection.
    #[tracing::instrument(skip_all)]
    pub fn render_with_target_selection_reserved_output(
        &mut self,
        device: &Device,
        queue: &Queue,
        tile_aabb: IRect,
        vertices: &[Vec2],
        // Must be in counter-clockwise order
        indices: &[u32],
        reserved_output_tiles: IndexSet<IVec2>,
    ) -> Option<DynamicLayerStorage> {
        self.render_internal(
            device,
            queue,
            tile_aabb,
            vertices,
            indices,
            reserved_output_tiles,
        )
    }

    fn render_internal(
        &mut self,
        device: &Device,
        queue: &Queue,
        tile_aabb: IRect,
        vertices: &[Vec2],
        // Must be in counter-clockwise order
        indices: &[u32],
        reserved_output_tiles: IndexSet<IVec2>,
    ) -> Option<DynamicLayerStorage> {
        if indices.is_empty() || vertices.is_empty() || tile_aabb.is_empty() {
            return None;
        }

        assert_eq!(indices.len() % 3, 0);

        let vertices_buffer = device.create_buffer_init(&BufferInitDescriptor {
            label: Some("selection_vertices_buffer"),
            contents: bytemuck::cast_slice(vertices),
            usage: BufferUsages::VERTEX,
        });

        let indices_buffer = device.create_buffer_init(&BufferInitDescriptor {
            label: Some("selection_indices_buffer"),
            contents: bytemuck::cast_slice(indices),
            usage: BufferUsages::INDEX,
        });

        let n_render_tiles = (tile_aabb.max - tile_aabb.min).element_product() as usize;

        let mut cur_rendering_indices = Vec::with_capacity(n_render_tiles);
        let mut cur_rendering_index_buffer =
            DynamicBuffer::new(Some("cur_tile_index_buffer"), BufferUsages::UNIFORM);
        let mut cur_rendering_index_offsets = Vec::with_capacity(n_render_tiles);

        let mut output_tiles = reserved_output_tiles;

        for x in tile_aabb.min.x..tile_aabb.max.x {
            for y in tile_aabb.min.y..tile_aabb.max.y {
                let index = IVec2 { x, y };

                cur_rendering_indices.push(index);
                let offset = cur_rendering_index_buffer.push(&index);
                cur_rendering_index_offsets.push(offset);
                output_tiles.insert(index);
            }
        }

        cur_rendering_index_buffer.write_buffer(device, queue);

        let mut selection = DynamicLayerStorage::new(
            device.clone().into(),
            queue.clone().into(),
            GpuLayerInfo {
                texel_type: self.layer_format,
            },
        );

        for tile in &output_tiles {
            selection.get_tile_or_allocate(*tile);
        }

        let render_bind_group = device.create_bind_group(&BindGroupDescriptor {
            label: Some("selection_render_bind_group"),
            layout: &self.render_layout,
            entries: &[BindGroupEntry {
                binding: 0,
                resource: cur_rendering_index_buffer.binding().unwrap(),
            }],
        });

        let mut ec = device.create_command_encoder(&Default::default());

        for (tile_index, index_buffer_offset) in cur_rendering_indices
            .into_iter()
            .zip(cur_rendering_index_offsets)
        {
            let target_view = selection.get_tile(tile_index).unwrap();

            let mut pass = ec.begin_render_pass(&RenderPassDescriptor {
                label: Some(&format!(
                    "selection_render_pass_x{}_y{}",
                    tile_index.x, tile_index.y
                )),
                color_attachments: &[Some(RenderPassColorAttachment {
                    view: &target_view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: Operations {
                        load: LoadOp::Load,
                        store: StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&self.render_pipeline);
            pass.set_bind_group(0, &render_bind_group, &[index_buffer_offset as u32]);
            pass.set_vertex_buffer(0, vertices_buffer.slice(..));
            pass.set_index_buffer(indices_buffer.slice(..), IndexFormat::Uint32);
            pass.draw_indexed(0..indices.len() as u32, 0, 0..1);
        }

        queue.submit([ec.finish()]);

        Some(selection)
    }

    /// Composite the input selection with the target selection. The input selection is allowed to unable
    /// to cover the target selection.
    ///
    /// The returned mask has no empty tiles.
    #[tracing::instrument(skip_all)]
    pub fn composite_with_tight_input(
        &self,
        device: &Device,
        queue: &Queue,
        op: SelectionOperation,
        input_selection: &DynamicLayerStorage,
        target_selection: &DynamicLayerStorage,
        target_selection_binding: &LayerBindingData,
    ) -> Option<DynamicLayerStorage> {
        let mut output_tiles = input_selection.deep_clone();
        for (tile, _, _) in target_selection.iter_tiles() {
            output_tiles.get_tile_or_allocate(tile);
        }

        self.composite_with_target_selection_reserved_input(
            device,
            queue,
            op,
            &output_tiles,
            target_selection_binding,
        )
    }

    /// Blend the selection from inout texture with target selection, then write it back to inout_texture.
    /// The inout texture must ensure it contains the area of target selection. Otherwise the result will
    /// be incomplete.
    ///
    /// The returned mask has no empty tiles.
    #[tracing::instrument(skip_all)]
    pub fn composite_with_target_selection_reserved_input(
        &self,
        device: &Device,
        queue: &Queue,
        op: SelectionOperation,
        input_selection: &DynamicLayerStorage,
        target_selection: &LayerBindingData,
    ) -> Option<DynamicLayerStorage> {
        let output_selection = input_selection.deep_clone();

        let composite_params_buffer = {
            let mut b = DynamicBuffer::new(Some("selection_params_buffer"), BufferUsages::UNIFORM);
            b.push(&SelectionParams {
                operation_ty: op as u32,
            });
            b.write_buffer(device, queue);
            b
        };

        let composite_bind_group = device.create_bind_group(&BindGroupDescriptor {
            label: Some("selection_composite_bind_group"),
            layout: &self.composite_layout,
            entries: &[
                BindGroupEntry {
                    binding: 0,
                    resource: BindingResource::TextureView(&target_selection.texture),
                },
                BindGroupEntry {
                    binding: 1,
                    resource: target_selection.tile_info_buffer.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 2,
                    resource: BindingResource::TextureView(output_selection.texture().unwrap()),
                },
                BindGroupEntry {
                    binding: 3,
                    resource: output_selection
                        .tile_info_buffer()
                        .unwrap()
                        .as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 4,
                    resource: composite_params_buffer.binding().unwrap(),
                },
            ],
        });

        let mut ec = device.create_command_encoder(&Default::default());

        {
            let mut pass = ec.begin_compute_pass(&ComputePassDescriptor {
                label: Some("selection_composite_pass"),
                ..Default::default()
            });

            pass.set_pipeline(&self.composite_pipeline);
            pass.set_bind_group(0, &composite_bind_group, &[]);
            pass.dispatch_workgroups(
                GpuTileStorageInner::TILE_SIZE.div_ceil(16),
                GpuTileStorageInner::TILE_SIZE.div_ceil(16),
                output_selection.len() as u32,
            );
        }

        queue.submit([ec.finish()]);

        Some(output_selection)
    }
}

#[derive(ShaderType, Debug, Clone, Copy)]
pub struct CanvasUniform {
    pub pixel_to_widget: Mat3,
    pub widget_min: Vec2,
    pub screen_size: Vec2,
    pub time: f32,
}

pub struct SelectionPreviewPipeline {
    layout: BindGroupLayout,
    pipeline: RenderPipeline,
}

impl SelectionPreviewPipeline {
    pub fn new(device: &Device, surface_format: TextureFormat) -> Self {
        let layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some("selection_preview_layout"),
            entries: &[
                BindGroupLayoutEntry {
                    binding: 0,
                    visibility: ShaderStages::VERTEX | ShaderStages::FRAGMENT,
                    ty: BindingType::Buffer {
                        ty: BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: Some(CanvasUniform::min_size()),
                    },
                    count: None,
                },
                BindGroupLayoutEntry {
                    binding: 1,
                    visibility: ShaderStages::VERTEX,
                    ty: BindingType::Buffer {
                        ty: BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: Some(Vec2::min_size()),
                    },
                    count: None,
                },
            ],
        });

        let pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: Some("selection_preview_pipeline_layout"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });

        let shader = device.create_shader_module(ShaderModuleDescriptor {
            label: Some("selection_preview_shader"),
            source: ShaderSource::Wgsl(include_wesl!("preview").into()),
        });

        let pipeline = device.create_render_pipeline(&RenderPipelineDescriptor {
            label: Some("selection_preview_pipeline"),
            layout: Some(&pipeline_layout),
            vertex: VertexState {
                module: &shader,
                entry_point: Some("vertex"),
                compilation_options: Default::default(),
                buffers: &[],
            },

            fragment: Some(FragmentState {
                module: &shader,
                entry_point: Some("fragment"),
                compilation_options: Default::default(),
                targets: &[Some(ColorTargetState {
                    format: surface_format,
                    blend: None,
                    write_mask: ColorWrites::ALL,
                })],
            }),
            primitive: Default::default(),
            depth_stencil: Default::default(),
            multisample: Default::default(),
            multiview_mask: Default::default(),
            cache: Default::default(),
        });

        Self { layout, pipeline }
    }

    pub fn draw(
        &self,
        device: &Device,
        queue: &Queue,
        line_vertices_ps: &[Vec2],
        canvas_surface: &TextureView,
        canvas_transform: &CanvasTransform,
    ) {
        if line_vertices_ps.len() < 2 {
            return;
        }

        let vertices_buffer = device.create_buffer_init(&BufferInitDescriptor {
            label: Some("selection_preview_vertices"),
            contents: bytemuck::cast_slice(line_vertices_ps),
            usage: BufferUsages::STORAGE,
        });

        let screen_size = canvas_surface.texture().size();

        static FIRST_DRAW: OnceLock<Instant> = OnceLock::new();
        let canvas_params_buffer = {
            let mut b = DynamicBuffer::new(Some("canvas_params_buffer"), BufferUsages::UNIFORM);
            b.push(&CanvasUniform {
                pixel_to_widget: canvas_transform.pixel_to_widget,
                widget_min: canvas_transform.widget_bounds.min,
                screen_size: Vec2::new(screen_size.width as f32, screen_size.height as f32),
                time: FIRST_DRAW.get_or_init(Instant::now).elapsed().as_secs_f32(),
            });
            b.write_buffer(device, queue);
            b.into_inner_buffer().unwrap()
        };

        let bind_group = device.create_bind_group(&BindGroupDescriptor {
            label: Some("canvas_params_bind_group"),
            layout: &self.layout,
            entries: &[
                BindGroupEntry {
                    binding: 0,
                    resource: canvas_params_buffer.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 1,
                    resource: vertices_buffer.as_entire_binding(),
                },
            ],
        });

        let mut ec = device.create_command_encoder(&Default::default());

        {
            let mut pass = ec.begin_render_pass(&RenderPassDescriptor {
                label: Some("selection_preview_pass"),
                color_attachments: &[Some(RenderPassColorAttachment {
                    view: canvas_surface,
                    depth_slice: None,
                    resolve_target: None,
                    ops: Operations {
                        load: LoadOp::Load,
                        store: StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });

            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &bind_group, &[]);
            pass.draw(0..(line_vertices_ps.len() as u32 - 1) * 6, 0..1);
        }

        queue.submit([ec.finish()]);
    }
}

pub(crate) fn indices_from_vertices(
    vertices: &[Vec2],
    fill_rule: FillRule,
) -> VertexBuffers<Vec2, u32> {
    let mut builder = Path::builder();
    builder.begin(lyon::geom::point(vertices[0].x, vertices[0].y));
    for v in &vertices[1..] {
        builder.line_to(lyon::geom::point(v.x, v.y));
    }
    builder.end(true);
    let path = builder.build();

    let mut geometry = VertexBuffers::new();
    let mut tessellator = FillTessellator::new();

    let options = FillOptions::default().with_fill_rule(fill_rule);

    tessellator
        .tessellate_path(
            &path,
            &options,
            &mut BuffersBuilder::new(&mut geometry, |v: FillVertex| {
                Vec2::new(v.position().x, v.position().y)
            }),
        )
        .unwrap();

    geometry
}
