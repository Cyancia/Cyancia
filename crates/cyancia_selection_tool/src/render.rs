use bevy_math::IRect;
use cyancia_image::{
    texel::TexelType,
    tile::{DynamicLayerStorage, GpuTileInfo, GpuTileStorageInner, LayerBindingData},
};
use cyancia_render::buffer::{BufferVec, DynamicBuffer};
use encase::ShaderType;
use glam::{IVec2, Mat2, Mat3, Vec2, Vec3};
use gpui::Global;
use wesl::include_wesl;
use wgpu::{
    BindGroupDescriptor, BindGroupEntry, BindGroupLayout, BindGroupLayoutDescriptor,
    BindGroupLayoutEntry, BindingResource, BindingType, BufferBindingType, BufferDescriptor,
    BufferUsages, Color, ColorTargetState, ColorWrites, ComputePassDescriptor, ComputePipeline,
    ComputePipelineDescriptor, Device, Extent3d, FragmentState, IndexFormat, LoadOp, Operations,
    PipelineLayoutDescriptor, Queue, RenderPassColorAttachment, RenderPassDescriptor,
    RenderPipeline, RenderPipelineDescriptor, ShaderModuleDescriptor, ShaderSource, ShaderStages,
    StorageTextureAccess, StoreOp, Texture, TextureDimension, TextureUsages, TextureViewDimension,
    VertexAttribute, VertexBufferLayout, VertexFormat, VertexState, VertexStepMode,
    util::{BufferInitDescriptor, DeviceExt},
    wgt::{TextureDescriptor, TextureViewDescriptor},
};

#[derive(Debug, Clone, Copy)]
#[repr(u32)]
pub enum SelectionOperation {
    And,
    Or,
    Xor,
    Diff,
}

#[derive(ShaderType, Debug, Clone, Copy)]
struct SelectionParams {
    operation_ty: u32,
}

pub struct SelectionPipeline {
    render_layout: BindGroupLayout,
    render_pipeline: RenderPipeline,
    composite_layout: BindGroupLayout,
    composite_pipeline: ComputePipeline,
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
                    array_stride: IVec2::min_size().into(),
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
        }
    }

    #[tracing::instrument(name = "draw_selection_mesh",skip_all)]
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
    ) -> Option<(Texture, Vec<IVec2>)> {
        if indices.is_empty() || vertices.is_empty() || tile_aabb.is_empty() {
            return None;
        }

        let composite_params_buffer = {
            let mut b = DynamicBuffer::new(Some("selection_params_buffer"), BufferUsages::UNIFORM);
            b.push(&SelectionParams {
                operation_ty: op as u32,
            });
            b.write_buffer(device, queue);
            b
        };

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

        let mut output_tile_info_buffer = BufferVec::new(
            Some("selection_output_tile_info_buffer".to_string()),
            BufferUsages::STORAGE,
        );

        let n_tiles = (tile_aabb.max - tile_aabb.min).element_product() as usize;
        let mut cur_tile_index_buffer =
            DynamicBuffer::new(Some("cur_tile_index_buffer"), BufferUsages::UNIFORM);
        let mut cur_tile_index_buffer_offsets = Vec::with_capacity(n_tiles);
        let mut output_tiles = Vec::with_capacity(n_tiles);

        for x in tile_aabb.min.x..tile_aabb.max.x {
            for y in tile_aabb.min.y..tile_aabb.max.y {
                let index = IVec2 { x, y };

                let offset = cur_tile_index_buffer.push(&index);
                cur_tile_index_buffer_offsets.push(offset);
                output_tile_info_buffer.push(&GpuTileInfo {
                    index,
                    origin: index * GpuTileStorageInner::TILE_SIZE as i32,
                });
                output_tiles.push(index);
            }
        }

        cur_tile_index_buffer.write_buffer(device, queue);
        output_tile_info_buffer.write_buffer(device, queue);
        let output_tile_info_buffer = output_tile_info_buffer.into_inner_buffer().unwrap();

        let output_buffer = device.create_texture(&TextureDescriptor {
            label: Some("selection_output_buffer"),
            size: Extent3d {
                width: GpuTileStorageInner::TILE_SIZE,
                height: GpuTileStorageInner::TILE_SIZE,
                depth_or_array_layers: n_tiles as u32,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: TextureDimension::D2,
            format: target_selection.texture.texture().format(),
            usage: TextureUsages::RENDER_ATTACHMENT
                | TextureUsages::STORAGE_BINDING
                | TextureUsages::COPY_SRC,
            view_formats: &[],
        });

        let render_bind_group = device.create_bind_group(&BindGroupDescriptor {
            label: Some("selection_render_bind_group"),
            layout: &self.render_layout,
            entries: &[BindGroupEntry {
                binding: 0,
                resource: cur_tile_index_buffer.binding().unwrap(),
            }],
        });

        let mut ec = device.create_command_encoder(&Default::default());

        for i in 0..n_tiles {
            let target_view = output_buffer.create_view(&TextureViewDescriptor {
                base_array_layer: i as u32,
                array_layer_count: Some(1),
                ..Default::default()
            });
            let mut pass = ec.begin_render_pass(&RenderPassDescriptor {
                label: Some(&format!("selection_render_pass_{}", i)),
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
            pass.set_bind_group(
                0,
                &render_bind_group,
                &[cur_tile_index_buffer_offsets[i] as u32],
            );
            pass.set_vertex_buffer(0, vertices_buffer.slice(..));
            pass.set_index_buffer(indices_buffer.slice(..), IndexFormat::Uint32);
            pass.draw_indexed(0..indices.len() as u32, 0, 0..1);
        }

        let output_buffer_view = output_buffer.create_view(&TextureViewDescriptor {
            dimension: Some(TextureViewDimension::D2Array),
            ..Default::default()
        });

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
                    resource: BindingResource::TextureView(&output_buffer_view),
                },
                BindGroupEntry {
                    binding: 3,
                    resource: output_tile_info_buffer.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 4,
                    resource: composite_params_buffer.binding().unwrap(),
                },
            ],
        });

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
                n_tiles as u32,
            );
        }

        unsafe { device.start_graphics_debugger_capture() };
        queue.submit([ec.finish()]);
        unsafe { device.stop_graphics_debugger_capture() };

        Some((output_buffer, output_tiles))
    }
}
