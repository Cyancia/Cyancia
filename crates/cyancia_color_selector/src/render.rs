use bytemuck::{Pod, Zeroable};
use cyancia_render::{
    bind_group_entries::BindGroupEntries,
    bind_group_layout_entries::{BindGroupLayoutEntries, binding_types},
    buffer::DynamicBuffer,
};
use encase::ShaderType;
use glam::{Mat3, Vec2, Vec3, Vec4};
use moxcms::ColorProfile;
use wesl::include_wesl;
use wgpu::{
    BindGroupDescriptor, BindGroupLayout, BindGroupLayoutDescriptor, Buffer, BufferUsages, Color,
    ColorTargetState, ColorWrites, Device, FragmentState, IndexFormat, LoadOp, Operations,
    PipelineLayoutDescriptor, Queue, RenderPassColorAttachment, RenderPassDescriptor,
    RenderPipeline, RenderPipelineDescriptor, ShaderModuleDescriptor, ShaderSource, ShaderStages,
    StoreOp, TextureFormat, TextureView, VertexAttribute, VertexBufferLayout, VertexFormat,
    VertexState, VertexStepMode,
    util::{BufferInitDescriptor, DeviceExt},
};

use crate::{
    GradientPlaneShape,
    config::{GradientBarConfig, GradientPlaneConfig},
};

pub struct GradientPipeline {
    layout: BindGroupLayout,
    pipeline: RenderPipeline,
}

impl GradientPipeline {
    pub fn new(device: &Device) -> Self {
        let layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some("gradient_layout"),
            entries: BindGroupLayoutEntries::sequential(
                ShaderStages::VERTEX_FRAGMENT,
                (binding_types::uniform_buffer::<GradientSettings>(false),),
            )
            .as_ref(),
        });

        let shader = device.create_shader_module(ShaderModuleDescriptor {
            label: Some("gradient_shader"),
            source: ShaderSource::Wgsl(include_wesl!("gradient").into()),
        });

        let pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: Some("gradient_pipeline_layout"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });

        let pipeline = device.create_render_pipeline(&RenderPipelineDescriptor {
            label: Some("gradient_pipeline"),
            layout: Some(&pipeline_layout),
            vertex: VertexState {
                module: &shader,
                entry_point: Some("vertex"),
                compilation_options: Default::default(),
                buffers: &[VertexBufferLayout {
                    array_stride: 16,
                    step_mode: VertexStepMode::Vertex,
                    attributes: &[
                        VertexAttribute {
                            format: VertexFormat::Float32x2,
                            offset: 0,
                            shader_location: 0,
                        },
                        VertexAttribute {
                            format: VertexFormat::Float32x2,
                            offset: 8,
                            shader_location: 1,
                        },
                    ],
                }],
            },
            fragment: Some(FragmentState {
                module: &shader,
                entry_point: Some("fragment"),
                compilation_options: Default::default(),
                targets: &[Some(ColorTargetState {
                    format: TextureFormat::Rgba16Float,
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
        mesh: &GradientMesh,
        settings: &GradientSettings,
        output: &TextureView,
        preserve_output: bool,
    ) {
        let mut settings_buffer = DynamicBuffer::new(
            Some("gradient_settings_buffer".into()),
            BufferUsages::UNIFORM,
        );
        settings_buffer.push(settings);
        settings_buffer.write_buffer(device, queue);

        let bind_group = device.create_bind_group(&BindGroupDescriptor {
            label: Some("gradient_bind_group"),
            layout: &self.layout,
            entries: BindGroupEntries::sequential((settings_buffer.binding().unwrap(),)).as_ref(),
        });

        let mut ec = device.create_command_encoder(&Default::default());

        {
            let mut pass = ec.begin_render_pass(&RenderPassDescriptor {
                label: Some("gradient_pass"),
                color_attachments: &[Some(RenderPassColorAttachment {
                    view: output,
                    depth_slice: None,
                    resolve_target: None,
                    ops: Operations {
                        load: if preserve_output {
                            LoadOp::Load
                        } else {
                            LoadOp::Clear(Color::TRANSPARENT)
                        },
                        store: StoreOp::Store,
                    },
                })],
                ..Default::default()
            });

            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &bind_group, &[]);
            pass.set_vertex_buffer(0, mesh.vertices.slice(..));
            pass.set_index_buffer(mesh.indices.slice(..), IndexFormat::Uint16);
            pass.draw_indexed(0..mesh.n_indices, 0, 0..1);
        }

        queue.submit([ec.finish()]);
    }
}

pub struct GradientRingPipeline {
    layout: BindGroupLayout,
    pipeline: RenderPipeline,
    mesh: GradientMesh,
}

impl GradientRingPipeline {
    pub fn new(device: &Device) -> Self {
        let layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some("gradient_ring_layout"),
            entries: BindGroupLayoutEntries::sequential(
                ShaderStages::FRAGMENT,
                (binding_types::uniform_buffer::<GradientSettings>(false),),
            )
            .as_ref(),
        });

        let shader = device.create_shader_module(ShaderModuleDescriptor {
            label: Some("gradient_ring_shader"),
            source: ShaderSource::Wgsl(include_wesl!("gradient").into()),
        });
        let pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: Some("gradient_ring_pipeline_layout"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&RenderPipelineDescriptor {
            label: Some("gradient_ring_pipeline"),
            layout: Some(&pipeline_layout),
            vertex: VertexState {
                module: &shader,
                entry_point: Some("ring_vertex"),
                compilation_options: Default::default(),
                buffers: &[VertexBufferLayout {
                    array_stride: 16,
                    step_mode: VertexStepMode::Vertex,
                    attributes: &[
                        VertexAttribute {
                            format: VertexFormat::Float32x2,
                            offset: 0,
                            shader_location: 0,
                        },
                        VertexAttribute {
                            format: VertexFormat::Float32x2,
                            offset: 8,
                            shader_location: 1,
                        },
                    ],
                }],
            },
            fragment: Some(FragmentState {
                module: &shader,
                entry_point: Some("ring_fragment"),
                compilation_options: Default::default(),
                targets: &[Some(ColorTargetState {
                    format: TextureFormat::Rgba16Float,
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
        let mesh = GradientMesh::new_plane(device, GradientPlaneShape::Square, 1.0);

        Self {
            layout,
            pipeline,
            mesh,
        }
    }

    pub fn draw(
        &self,
        device: &Device,
        queue: &Queue,
        settings: &GradientSettings,
        output: &TextureView,
    ) {
        let mut settings_buffer = DynamicBuffer::new(
            Some("gradient_ring_settings_buffer".into()),
            BufferUsages::UNIFORM,
        );
        settings_buffer.push(settings);
        settings_buffer.write_buffer(device, queue);

        let bind_group = device.create_bind_group(&BindGroupDescriptor {
            label: Some("gradient_ring_bind_group"),
            layout: &self.layout,
            entries: BindGroupEntries::sequential((settings_buffer.binding().unwrap(),)).as_ref(),
        });
        let mut encoder = device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
                label: Some("gradient_ring_pass"),
                color_attachments: &[Some(RenderPassColorAttachment {
                    view: output,
                    depth_slice: None,
                    resolve_target: None,
                    ops: Operations {
                        load: LoadOp::Clear(Color::TRANSPARENT),
                        store: StoreOp::Store,
                    },
                })],
                ..Default::default()
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &bind_group, &[]);
            pass.set_vertex_buffer(0, self.mesh.vertices.slice(..));
            pass.set_index_buffer(self.mesh.indices.slice(..), IndexFormat::Uint16);
            pass.draw_indexed(0..self.mesh.n_indices, 0, 0..1);
        }

        queue.submit([encoder.finish()]);
    }
}

#[derive(Pod, Zeroable, Clone, Copy)]
#[repr(C)]
struct Vertex {
    position: Vec2,
    uv: Vec2,
}

pub struct GradientMesh {
    n_indices: u32,
    vertices: Buffer,
    indices: Buffer,
}

impl GradientMesh {
    pub fn new_bar(device: &Device) -> Self {
        Self::from_vertices(
            device,
            vec![
                vtx(-1.0, -1.0, 0.0, 0.0),
                vtx(1.0, -1.0, 1.0, 0.0),
                vtx(1.0, 1.0, 1.0, 0.0),
                vtx(-1.0, 1.0, 0.0, 0.0),
            ],
            vec![0, 1, 2, 2, 3, 0],
        )
    }

    pub fn new_plane(device: &Device, shape: GradientPlaneShape, scale: f32) -> Self {
        let (mut vertices, indices) = match shape {
            GradientPlaneShape::Square => (
                vec![
                    vtx(-1.0, -1.0, 0.0, 0.0),
                    vtx(1.0, -1.0, 1.0, 0.0),
                    vtx(1.0, 1.0, 1.0, 1.0),
                    vtx(-1.0, 1.0, 0.0, 1.0),
                ],
                vec![0, 1, 2, 2, 3, 0],
            ),
            GradientPlaneShape::Triangle => (
                vec![
                    vtx(0.0, 1.0, 0.5, 0.0),
                    vtx(-3.0_f32.sqrt() * 0.5, -0.5, 0.0, 1.0),
                    vtx(3.0_f32.sqrt() * 0.5, -0.5, 1.0, 1.0),
                ],
                vec![0, 1, 2],
            ),
        };

        for vertex in &mut vertices {
            vertex.position *= scale;
        }
        Self::from_vertices(device, vertices, indices)
    }

    fn from_vertices(device: &Device, vertices: Vec<Vertex>, indices: Vec<u16>) -> Self {
        Self {
            n_indices: indices.len() as u32,
            vertices: device.create_buffer_init(&BufferInitDescriptor {
                label: Some("gradient_vertex_buffer"),
                contents: bytemuck::cast_slice(&vertices),
                usage: BufferUsages::VERTEX,
            }),
            indices: device.create_buffer_init(&BufferInitDescriptor {
                label: Some("gradient_index_buffer"),
                contents: bytemuck::cast_slice(&indices),
                usage: BufferUsages::INDEX,
            }),
        }
    }
}

#[inline]
fn vtx(px: f32, py: f32, uvx: f32, uvy: f32) -> Vertex {
    Vertex {
        position: Vec2::new(px, py),
        uv: Vec2::new(uvx, uvy),
    }
}

#[derive(ShaderType, Clone)]
pub struct GradientSettings {
    rgb_to_xyz: Mat3,
    xyz_to_rgb: Mat3,
    channel_ranges: [Vec4; 3],
    base_channels: Vec3,
    color_model: u32,
    variable_channels: u32,
    rotation: f32,
    flip_axis: u32,
    primary_channel: u32,
    ring_rotation: f32,
    reversed_ring: u32,
    saturate_primary_channel: u32,
    ring_width: f32,
    texture_size: f32,
}

impl GradientSettings {
    pub fn new_plane(
        profile: &ColorProfile,
        base_channels: Vec3,
        config: &GradientPlaneConfig,
        primary_channel_override: Option<u8>,
        texture_size: f32,
    ) -> Self {
        let variable_channels = primary_channel_override
            .map_or(config.variable_channels, |channel| 0b111 & !(1 << channel));
        let matrix = profile.rgb_to_xyz_matrix().to_f32().v;
        let rgb_to_xyz = Mat3::from_cols_array(&[
            matrix[0][0],
            matrix[1][0],
            matrix[2][0],
            matrix[0][1],
            matrix[1][1],
            matrix[2][1],
            matrix[0][2],
            matrix[1][2],
            matrix[2][2],
        ]);

        Self {
            rgb_to_xyz,
            xyz_to_rgb: rgb_to_xyz.inverse(),
            channel_ranges: config
                .model
                .channel_ranges()
                .map(|range| Vec4::new(range.x, range.y, 0.0, 0.0)),
            base_channels,
            color_model: config.model as u32,
            variable_channels: u32::from(variable_channels),
            rotation: config.rotation,
            flip_axis: u32::from(config.flip_axis.bits()),
            primary_channel: (0..3)
                .find(|channel| variable_channels & (1 << channel) == 0)
                .unwrap_or(0),
            ring_rotation: config.ring_rotation,
            reversed_ring: u32::from(config.reversed_ring),
            saturate_primary_channel: u32::from(config.saturated_primary_channel),
            ring_width: config.primary_channel_ring_width,
            texture_size,
        }
    }

    pub fn new_bar(
        profile: &ColorProfile,
        base_channels: Vec3,
        config: &GradientBarConfig,
        saturate_primary_channel: bool,
    ) -> Self {
        let matrix = profile.rgb_to_xyz_matrix().to_f32().v;
        let rgb_to_xyz = Mat3::from_cols_array(&[
            matrix[0][0],
            matrix[1][0],
            matrix[2][0],
            matrix[0][1],
            matrix[1][1],
            matrix[2][1],
            matrix[0][2],
            matrix[1][2],
            matrix[2][2],
        ]);

        Self {
            rgb_to_xyz,
            xyz_to_rgb: rgb_to_xyz.inverse(),
            channel_ranges: config
                .model
                .channel_ranges()
                .map(|range| Vec4::new(range.x, range.y, 0.0, 0.0)),
            base_channels,
            color_model: config.model as u32,
            variable_channels: 1 << config.channel,
            rotation: 0.0,
            flip_axis: 0,
            primary_channel: 0,
            ring_rotation: 0.0,
            reversed_ring: 0,
            saturate_primary_channel: u32::from(saturate_primary_channel),
            ring_width: 0.0,
            texture_size: 1.0,
        }
    }
}
