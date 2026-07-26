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

use crate::{ColorModel, GradientPlaneShape};

pub struct GradientPipeline {
    layout: BindGroupLayout,
    pipeline: RenderPipeline,
}

impl GradientPipeline {
    pub fn new(device: &Device) -> Self {
        let layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some("gradient_layout"),
            entries: BindGroupLayoutEntries::sequential(
                ShaderStages::FRAGMENT,
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
                        load: LoadOp::Clear(Color::TRANSPARENT),
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GradientShape {
    Bar,
    Square,
    Triangle,
}

impl From<GradientPlaneShape> for GradientShape {
    fn from(value: GradientPlaneShape) -> Self {
        match value {
            GradientPlaneShape::Square => Self::Square,
            GradientPlaneShape::Triangle => Self::Triangle,
        }
    }
}

#[derive(Pod, Zeroable, Clone, Copy)]
#[repr(C)]
pub struct Vertex {
    pub position: Vec2,
    pub uv: Vec2,
}

pub struct GradientMesh {
    pub n_indices: u32,
    pub vertices: Buffer,
    pub indices: Buffer,
}

impl GradientMesh {
    pub fn new(device: &Device, shape: GradientShape) -> Self {
        let (vertices, indices): (_, Vec<u16>) = match shape {
            GradientShape::Bar => (
                vec![
                    vtx(-1.0, -1.0, 0.0, 0.0),
                    vtx(1.0, -1.0, 1.0, 0.0),
                    vtx(1.0, 1.0, 1.0, 0.0),
                    vtx(-1.0, 1.0, 0.0, 0.0),
                ],
                vec![0, 1, 2, 2, 3, 0],
            ),
            GradientShape::Square => (
                vec![
                    vtx(-1.0, -1.0, 0.0, 0.0),
                    vtx(1.0, -1.0, 1.0, 0.0),
                    vtx(1.0, 1.0, 1.0, 1.0),
                    vtx(-1.0, 1.0, 0.0, 1.0),
                ],
                vec![0, 1, 2, 2, 3, 0],
            ),
            GradientShape::Triangle => (
                vec![
                    vtx(0.0, 1.0, 0.5, 0.0),
                    vtx(-1.0, -1.0, 0.0, 1.0),
                    vtx(1.0, -1.0, 1.0, 1.0),
                ],
                vec![0, 1, 2],
            ),
        };

        let vertex_buffer = device.create_buffer_init(&BufferInitDescriptor {
            label: Some("gradient_vertex_buffer"),
            contents: bytemuck::cast_slice(&vertices),
            usage: BufferUsages::VERTEX,
        });
        let index_buffer = device.create_buffer_init(&BufferInitDescriptor {
            label: Some("gradient_index_buffer"),
            contents: bytemuck::cast_slice(&indices),
            usage: BufferUsages::INDEX,
        });

        GradientMesh {
            n_indices: indices.len() as u32,
            vertices: vertex_buffer,
            indices: index_buffer,
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
    pub rgb_to_xyz: Mat3,
    pub xyz_to_rgb: Mat3,
    pub channel_ranges: [Vec4; 3],
    pub reference: Vec3,
    pub color_model: u32,
    pub variable_channels: u32,
}

impl GradientSettings {
    pub fn new(
        profile: &ColorProfile,
        reference: Vec3,
        color_model: ColorModel,
        variable_channels: u32,
    ) -> Self {
        let m = profile.rgb_to_xyz_matrix().to_f32().v;

        let rgb_to_xyz = Mat3::from_cols_array(&[
            m[0][0], m[1][0], m[2][0], m[0][1], m[1][1], m[2][1], m[0][2], m[1][2], m[2][2],
        ]);
        let xyz_to_rgb = rgb_to_xyz.inverse();

        let channel_ranges = color_model
            .channel_ranges()
            .map(|range| Vec4::new(range.x, range.y, 0.0, 0.0));

        Self {
            rgb_to_xyz,
            xyz_to_rgb,
            channel_ranges,
            reference,
            color_model: color_model as u32,
            variable_channels,
        }
    }
}
