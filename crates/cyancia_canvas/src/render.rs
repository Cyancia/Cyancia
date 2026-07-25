use std::{sync::OnceLock, time::Instant};

use bevy_math::IRect;
use cyancia_color::shader::IccTransformShader;
use cyancia_image::{
    layer::LayerId,
    texel::TexelType,
    tile::{GpuTileInfo, GpuTileStorage},
};
use cyancia_render::{
    bind_group_entries::BindGroupEntries,
    bind_group_layout_entries::{BindGroupLayoutEntries, binding_types},
    buffer::DynamicBuffer,
    wesl_jit::compile_wesl,
};
use encase::ShaderType;
use glam::{IVec2, Mat3, UVec2, UVec3};
use gpui::Global;
use wgpu::{
    BindGroup, BindGroupDescriptor, BindGroupLayout, BindGroupLayoutDescriptor, BindingResource,
    BufferUsages, CommandEncoder, ComputePassDescriptor, ComputePipeline,
    ComputePipelineDescriptor, Device, Extent3d, PipelineLayoutDescriptor, Queue,
    ShaderModuleDescriptor, ShaderSource, ShaderStages, StorageTextureAccess, TextureDescriptor,
    TextureDimension, TextureFormat, TextureUsages, TextureView, TextureViewDescriptor,
};

use crate::control::CanvasTransform;

/// When rendering canvas, we need to first compose all tiles onto a temporary surface.
/// This surface will be used as storage texture and float sampled texture.
pub const INTERMEDIATE_BUFFER_FORMAT: TextureFormat = TextureFormat::Rgba8Unorm;

pub const ICC_TRANSFORM_SHADER_IDENT: &str = "calibrate_color";

#[derive(Debug)]
pub struct CanvasRenderer {
    texture: Option<TextureView>,
    render_pipeline: CanvasRenderPipeline,
}

impl Global for CanvasRenderer {}

impl CanvasRenderer {
    pub fn new(
        device: &Device,
        root_texel_type: TexelType,
        selection_texel_type: TexelType,
        icc_transform: &IccTransformShader,
    ) -> Self {
        let render_pipeline =
            CanvasRenderPipeline::new(device, root_texel_type, selection_texel_type, icc_transform);
        Self {
            texture: None,
            render_pipeline,
            // present_pipeline,
        }
    }

    pub fn resize_output_buffer(&mut self, device: &Device, size: UVec2) {
        if self
            .texture
            .as_ref()
            .is_some_and(|t| t.texture().width() == size.x && t.texture().height() == size.y)
        {
            return;
        }

        let format = INTERMEDIATE_BUFFER_FORMAT;

        let texture = device.create_texture(&TextureDescriptor {
            label: Some("canvas render buffer"),
            size: Extent3d {
                width: size.x,
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

        self.texture = Some(texture_view);
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
        let Some(texture) = &self.texture else {
            return;
        };

        let tile_rect = GpuTileStorage::pixel_rect_to_tile(IRect {
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
                tile_size: GpuTileStorage::TILE_SIZE,
                time: FIRST_DRAW.get_or_init(Instant::now).elapsed().as_secs_f32(),
            },
            texture,
            tile_storage,
            root_layer_id,
            selection_layer_id,
        );
        // self.present_pipeline.prepare(&device, buffer);
    }

    pub fn draw(&self, device: &Device, queue: &Queue) {
        let mut ec = device.create_command_encoder(&Default::default());
        self.render_pipeline.draw(&mut ec);
        queue.submit([ec.finish()]);
    }

    pub fn texture(&self) -> Option<&TextureView> {
        self.texture.as_ref()
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
    fn new(
        device: &Device,
        root_texel_type: TexelType,
        selection_texel_type: TexelType,
        icc_transform: &IccTransformShader,
    ) -> Self {
        let main_layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some("canvas main layout"),
            entries: BindGroupLayoutEntries::sequential(
                ShaderStages::COMPUTE,
                (
                    // tiles
                    binding_types::texture_storage_2d_array(
                        root_texel_type.wgpu_format(),
                        StorageTextureAccess::ReadOnly,
                    ),
                    binding_types::storage_buffer_read_only::<GpuTileInfo>(false),
                    binding_types::uniform_buffer::<CanvasUniform>(false),
                    // output
                    binding_types::texture_storage_2d(
                        INTERMEDIATE_BUFFER_FORMAT,
                        StorageTextureAccess::WriteOnly,
                    ),
                    // selection
                    binding_types::texture_storage_2d_array(
                        selection_texel_type.wgpu_format(),
                        StorageTextureAccess::ReadOnly,
                    ),
                    binding_types::storage_buffer_read_only::<GpuTileInfo>(false),
                ),
            )
            .as_ref(),
        });

        let pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: Some("canvas pipeline layout"),
            bind_group_layouts: &[Some(&main_layout)],
            ..Default::default()
        });

        let shader = include_str!("shaders/canvas_render.wesl")
            .replace("//CODEGEN_FLAG_CALIBRATE_COLOR", &icc_transform.function);
        let shader_module = device.create_shader_module(ShaderModuleDescriptor {
            label: Some("canvas shader"),
            source: ShaderSource::Wgsl(
                compile_wesl(shader, &[cyancia_image::image::PACKAGE])
                    .unwrap()
                    .into(),
            ),
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
                Some("canvas uniform buffer".into()),
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
            entries: BindGroupEntries::sequential((
                BindingResource::TextureView(&root_layer.texture),
                root_layer.tile_info_buffer.as_entire_binding(),
                uniform_buffer,
                BindingResource::TextureView(target),
                BindingResource::TextureView(&selection_layer.texture),
                selection_layer.tile_info_buffer.as_entire_binding(),
            ))
            .as_ref(),
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
