use std::{
    sync::{Arc, OnceLock},
    time::Instant,
};

use bevy_math::IRect;
use cyancia_image::{
    layer::LayerId,
    texel::TexelType,
    tile::{GpuTileInfo, GpuTileStorage, GpuTileStorageInner},
};
use cyancia_render::buffer::DynamicBuffer;
use cyancia_utils::include_shader;
use encase::ShaderType;
use glam::{IVec2, Mat3, UVec2, UVec3};
use gpui::{Global, RenderImage};
use image::{Frame, RgbaImage};
use wgpu::{
    BindGroup, BindGroupDescriptor, BindGroupEntry, BindGroupLayout, BindGroupLayoutDescriptor,
    BindGroupLayoutEntry, BindingResource, BindingType, Buffer, BufferBindingType,
    BufferDescriptor, BufferUsages, COPY_BYTES_PER_ROW_ALIGNMENT, CommandEncoder,
    ComputePassDescriptor, ComputePipeline, ComputePipelineDescriptor, Device, Extent3d, MapMode,
    PipelineLayoutDescriptor, Queue, ShaderModuleDescriptor, ShaderSource, ShaderStages,
    StorageTextureAccess, SubmissionIndex, TexelCopyBufferInfo, TexelCopyBufferLayout,
    TextureDescriptor, TextureDimension, TextureFormat, TextureUsages, TextureView,
    TextureViewDescriptor, TextureViewDimension,
};

use crate::control::CanvasTransform;

/// When rendering canvas, we need to first compose all tiles onto a temporary surface.
/// This surface will be used as storage texture and float sampled texture.
pub const INTERMEDIATE_BUFFER_FORMAT: TextureFormat = TextureFormat::Rgba8Unorm;

#[derive(Debug)]
pub struct CanvasRenderer {
    buffer: Option<(TextureView, Buffer)>,
    render_pipeline: CanvasRenderPipeline,
}

impl Global for CanvasRenderer {}

impl CanvasRenderer {
    pub fn new(
        device: &Device,
        root_texel_type: TexelType,
        selection_texel_type: TexelType,
    ) -> Self {
        let render_pipeline =
            CanvasRenderPipeline::new(device, root_texel_type, selection_texel_type);
        Self {
            buffer: Default::default(),
            render_pipeline,
            // present_pipeline,
        }
    }

    pub fn resize_output_buffer(&mut self, device: &Device, size: UVec2) {
        if self
            .buffer
            .as_ref()
            .is_some_and(|(t, _)| t.texture().width() == size.x && t.texture().height() == size.y)
        {
            return;
        }

        let format = INTERMEDIATE_BUFFER_FORMAT;

        let texel_size = format.block_copy_size(None).unwrap();
        let aligned_bytes_per_row =
            (size.x * texel_size).next_multiple_of(COPY_BYTES_PER_ROW_ALIGNMENT);
        let aligned_width = aligned_bytes_per_row / texel_size;

        let texture = device.create_texture(&TextureDescriptor {
            label: Some("canvas render buffer"),
            size: Extent3d {
                width: aligned_width,
                height: size.y,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: TextureDimension::D2,
            format,
            usage: TextureUsages::RENDER_ATTACHMENT
                | TextureUsages::STORAGE_BINDING
                | TextureUsages::TEXTURE_BINDING
                | TextureUsages::COPY_SRC,
            view_formats: &[],
        });

        let texture_view = texture.create_view(&TextureViewDescriptor::default());
        let buffer = device.create_buffer(&BufferDescriptor {
            label: Some("canvas readback buffer"),
            size: (aligned_width * size.y * texel_size) as u64,
            usage: BufferUsages::COPY_DST | BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });

        self.buffer = Some((texture_view, buffer));
    }

    pub fn prepare(
        &mut self,
        device: &Device,
        queue: &Queue,
        canvas_transform: &CanvasTransform,
        image_size: UVec2,
        tile_storage: &GpuTileStorage,
        root_layer_id: LayerId,
        selection_layer_id: LayerId,
    ) {
        let Some((buffer, _)) = &self.buffer else {
            return;
        };

        let tile_rect = GpuTileStorageInner::pixel_rect_to_tile(IRect {
            min: IVec2::ZERO,
            max: image_size.as_ivec2(),
        });

        static FIRST_DRAW: OnceLock<Instant> = OnceLock::new();
        self.render_pipeline.prepare(
            device,
            queue,
            CanvasUniform {
                transform: canvas_transform.pixel_to_widget,
                inv_transform: canvas_transform.pixel_to_widget.inverse(),
                size: image_size,
                total_tile_count: tile_rect.size().as_uvec2(),
                tile_size: GpuTileStorageInner::TILE_SIZE,
                time: FIRST_DRAW.get_or_init(Instant::now).elapsed().as_secs_f32(),
            },
            buffer,
            tile_storage,
            root_layer_id,
            selection_layer_id,
        );
        // self.present_pipeline.prepare(&device, buffer);
    }

    pub fn draw(
        &self,
        device: &Device,
        queue: &Queue,
        post_draw: impl FnOnce(&TextureView),
    ) -> (
        SubmissionIndex,
        futures::channel::oneshot::Receiver<Arc<RenderImage>>,
    ) {
        let mut ec = device.create_command_encoder(&Default::default());
        self.render_pipeline.draw(&mut ec);
        queue.submit([ec.finish()]);

        // TODO Dirty workaround. Remove this once gpui supports wgpu backend on all platforms.
        post_draw(&self.buffer.as_ref().unwrap().0);

        let (texture, buffer) = self.buffer.as_ref().expect("buffer not initialized");
        let buffer = buffer.clone();
        let texture_size = texture.texture().size();
        let texel_size = texture.texture().format().block_copy_size(None).unwrap();

        let mut ec = device.create_command_encoder(&Default::default());

        ec.copy_texture_to_buffer(
            texture.texture().as_image_copy(),
            TexelCopyBufferInfo {
                buffer: &buffer,
                layout: TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(texture_size.width * texel_size),
                    rows_per_image: Some(texture_size.height),
                },
            },
            texture_size,
        );

        let (tx, rx) = futures::channel::oneshot::channel();
        let mapped_buffer = buffer.clone();
        let submission_index = queue.submit([ec.finish()]);

        buffer.slice(..).map_async(MapMode::Read, move |result| {
            result.unwrap();

            let buffer_slice = mapped_buffer.slice(..);
            let mapped = buffer_slice.get_mapped_range();
            // We are treating this texture as bgra texture in shader, so we can
            // convert it to RenderImage directly. See canvas_render.wesl
            let image =
                RgbaImage::from_raw(texture_size.width, texture_size.height, mapped.to_vec())
                    .unwrap();
            let render_image = RenderImage::new([Frame::new(image)]);
            drop(mapped);
            mapped_buffer.unmap();

            tx.send(Arc::new(render_image)).ok();
        });

        (submission_index, rx)
    }
}

#[derive(Debug)]
pub struct CanvasRenderPipeline {
    pipeline: ComputePipeline,
    main_layout: BindGroupLayout,
    uniform_buffer: DynamicBuffer<CanvasUniform>,
    uniform: Option<CanvasUniform>,
    dispatch: Option<(BindGroup, UVec3)>,
}

#[derive(Debug, Clone, Copy, ShaderType)]
pub struct CanvasUniform {
    pub transform: Mat3,
    pub inv_transform: Mat3,
    pub size: UVec2,
    pub total_tile_count: UVec2,
    pub tile_size: u32,
    pub time: f32,
}

impl CanvasRenderPipeline {
    fn new(device: &Device, root_texel_type: TexelType, selection_texel_type: TexelType) -> Self {
        let main_layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some("canvas main layout"),
            entries: &[
                // tiles
                BindGroupLayoutEntry {
                    binding: 0,
                    visibility: ShaderStages::COMPUTE,
                    ty: BindingType::StorageTexture {
                        access: StorageTextureAccess::ReadOnly,
                        format: root_texel_type.wgpu_format(),
                        view_dimension: TextureViewDimension::D2Array,
                    },
                    count: None,
                },
                // tile info
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
                // canvas uniform
                BindGroupLayoutEntry {
                    binding: 2,
                    visibility: ShaderStages::COMPUTE,
                    ty: BindingType::Buffer {
                        ty: BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: Some(<CanvasUniform as ShaderType>::min_size()),
                    },
                    count: None,
                },
                // output
                BindGroupLayoutEntry {
                    binding: 3,
                    visibility: ShaderStages::COMPUTE,
                    ty: BindingType::StorageTexture {
                        access: StorageTextureAccess::WriteOnly,
                        format: INTERMEDIATE_BUFFER_FORMAT,
                        view_dimension: TextureViewDimension::D2,
                    },
                    count: None,
                },
                BindGroupLayoutEntry {
                    binding: 4,
                    visibility: ShaderStages::COMPUTE,
                    ty: BindingType::StorageTexture {
                        access: StorageTextureAccess::ReadOnly,
                        format: selection_texel_type.wgpu_format(),
                        view_dimension: TextureViewDimension::D2Array,
                    },
                    count: None,
                },
                BindGroupLayoutEntry {
                    binding: 5,
                    visibility: ShaderStages::COMPUTE,
                    ty: BindingType::Buffer {
                        ty: BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: Some(GpuTileInfo::min_size()),
                    },
                    count: None,
                },
            ],
        });

        let pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: Some("canvas pipeline layout"),
            bind_group_layouts: &[Some(&main_layout)],
            ..Default::default()
        });

        let shader_module = device.create_shader_module(ShaderModuleDescriptor {
            label: Some("canvas shader"),
            source: ShaderSource::Wgsl(include_shader!("canvas_render.wgsl").into()),
        });

        let pipeline = device.create_compute_pipeline(&ComputePipelineDescriptor {
            label: Some("canvas pipeline"),
            layout: Some(&pipeline_layout),
            entry_point: Some("main"),
            module: &shader_module,
            compilation_options: Default::default(),
            cache: None,
        });

        Self {
            main_layout,
            pipeline,
            uniform_buffer: DynamicBuffer::new(
                Some("canvas uniform buffer"),
                BufferUsages::UNIFORM,
            ),
            uniform: None,
            dispatch: None,
        }
    }

    pub fn prepare(
        &mut self,
        device: &Device,
        queue: &Queue,
        uniform: CanvasUniform,
        target: &TextureView,
        tile_storage: &GpuTileStorage,
        root_layer_id: LayerId,
        selection_layer_id: LayerId,
    ) {
        self.uniform_buffer.clear();
        self.uniform_buffer.push(&uniform);
        self.uniform_buffer.write_buffer(device, queue);
        self.uniform = Some(uniform);

        let Some(uniform_buffer) = self.uniform_buffer.binding() else {
            return;
        };

        let root_layer = tile_storage
            .get_layer_binding_or_empty(root_layer_id)
            .unwrap();
        let selection_layer = tile_storage
            .get_layer_binding_or_empty(selection_layer_id)
            .unwrap();

        let bind_group = device.create_bind_group(&BindGroupDescriptor {
            label: Some("canvas render bind group"),
            layout: &self.main_layout,
            entries: &[
                BindGroupEntry {
                    binding: 0,
                    resource: BindingResource::TextureView(&root_layer.texture),
                },
                BindGroupEntry {
                    binding: 1,
                    resource: root_layer.tile_info_buffer.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 2,
                    resource: uniform_buffer,
                },
                BindGroupEntry {
                    binding: 3,
                    resource: BindingResource::TextureView(target),
                },
                BindGroupEntry {
                    binding: 4,
                    resource: BindingResource::TextureView(&selection_layer.texture),
                },
                BindGroupEntry {
                    binding: 5,
                    resource: selection_layer.tile_info_buffer.as_entire_binding(),
                },
            ],
        });

        let target_size = target.texture().size();
        self.dispatch = Some((
            bind_group,
            UVec3::new(
                target_size.width.div_ceil(16),
                target_size.height.div_ceil(16),
                1,
            ),
        ));
    }

    fn draw(&self, encoder: &mut CommandEncoder) {
        let Some((bind_group, workgroups)) = &self.dispatch else {
            return;
        };

        let mut pass = encoder.begin_compute_pass(&ComputePassDescriptor {
            label: Some("canvas render pass"),
            timestamp_writes: None,
        });

        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, bind_group, &[]);
        pass.dispatch_workgroups(workgroups.x, workgroups.y, workgroups.z);
    }
}
