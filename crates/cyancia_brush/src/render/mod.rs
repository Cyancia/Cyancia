use std::{num::NonZeroU32, sync::Arc};

use cyancia_assets::{asset::AssetId, store::AssetRegistry};
use cyancia_image::{
    layer::LayerId,
    texel::TexelType,
    tile::{GpuTileStorage, GpuTileStorageInner, Tile},
};
use cyancia_render::buffer::{BufferVec, DynamicBuffer};
use cyancia_shader_graph::{
    graph::node::external::generate_external_variable_binding,
    wgsl_std::nodes::{TextureId, TextureUsageRecorder},
};
use encase::ShaderType;
use glam::{IVec2, UVec2, Vec2};
use uuid::Uuid;
use wesl::{VirtualResolver, Wesl};
use wgpu::{
    BindGroup, BindGroupDescriptor, BindGroupEntry, BindGroupLayout, BindGroupLayoutDescriptor,
    BindGroupLayoutEntry, BindingResource, BindingType, Buffer, BufferBindingType, BufferUsages,
    ComputePipeline, ComputePipelineDescriptor, Device, Extent3d, PipelineLayoutDescriptor, Queue,
    ShaderModuleDescriptor, ShaderSource, ShaderStages, StorageTextureAccess, Texture,
    TextureDescriptor, TextureDimension, TextureFormat, TextureSampleType, TextureUsages,
    TextureView, TextureViewDimension,
    naga::StorageAccess,
    util::{BufferInitDescriptor, DeviceExt},
};

use crate::{
    asset::{BrushPresetInstance, GpuImage},
    render::graph::GraphInputParams,
};

pub mod graph;

pub fn compile_brush_wesl(shader: String) -> anyhow::Result<String> {
    let mut resolver = VirtualResolver::new();
    resolver.add_module("template".parse().unwrap(), shader.into());
    resolver.add_module(
        "template/image::texture_unpack".parse().unwrap(),
        include_str!("../../../cyancia_image/src/shaders/texture_unpack.wesl").into(),
    );
    resolver.add_module(
        "template/image::blend_modes".parse().unwrap(),
        include_str!("../../../cyancia_image/src/shaders/blend_modes.wesl").into(),
    );
    let mut compiler = Wesl::new_barebones().set_custom_resolver(resolver);
    compiler.set_mangler(Default::default());
    compiler.set_options(Default::default());

    let shader = compiler.compile(&"template".parse().unwrap())?;
    Ok(shader.to_string())
}

pub const BRUSH_RENDER_TARGET: LayerId =
    LayerId::new(Uuid::from_u128(85004653408671049643065089641532));

pub struct BrushPresetOperator {
    instance: BrushPresetInstance,
    renderer: BrushPresetRenderer,
}

impl BrushPresetOperator {
    pub fn new(instance: BrushPresetInstance, device: Arc<Device>, queue: Arc<Queue>) -> Self {
        let renderer = BrushPresetRenderer::new(device, queue);
        Self { instance, renderer }
    }

    pub fn prepare(
        &mut self,
        params: GraphInputParams,
        output_layer: LayerId,
        tiles: &GpuTileStorage,
        assets: &AssetRegistry,
    ) {
        let target_layer_texel = tiles.layer_texel_type(output_layer).unwrap();
        if self
            .renderer
            .initialized
            .as_ref()
            .is_none_or(|initialized| {
                let estimated_size = self.instance.estimate_size();
                initialized.estimated_size != estimated_size
                    || initialized.target_layer_texel != target_layer_texel
            })
        {
            self.renderer
                .initialize(&mut self.instance, assets, target_layer_texel);
        }

        self.renderer
            .prepare(&mut self.instance, params, output_layer, tiles);
    }

    pub fn draw(&self) {
        self.renderer.draw();
    }
}

#[derive(ShaderType, Debug)]
pub struct GraphInputUniform {
    pub shader_origin: IVec2,
    pub estimated_brush_size: UVec2,
    pub pen_position: Vec2,
    pub tile_size: u32,
}

#[derive(ShaderType)]
pub struct TileInfo {
    pub tile_origin: IVec2,
}

struct InitializedData {
    estimated_size: UVec2,
    output_len_in_layout: u32,
    pipeline: ComputePipeline,
    main_layout: BindGroupLayout,
    target_layer_texel: TexelType,
    textures: Vec<TextureView>,
}

pub struct BrushPresetRenderer {
    device: Arc<Device>,
    queue: Arc<Queue>,
    graph_input: DynamicBuffer<GraphInputUniform>,

    initialized: Option<InitializedData>,
    main_bind_group: Option<BindGroup>,
    external_var_buffers: Vec<Buffer>,
    tile_info: BufferVec<TileInfo>,

    empty_texture: GpuImage,
}

impl BrushPresetRenderer {
    pub fn new(device: Arc<Device>, queue: Arc<Queue>) -> Self {
        let empty_texture = device.create_texture(&TextureDescriptor {
            label: None,
            size: Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: TextureDimension::D2,
            // Random texture that can be binded as texture_2d<f32>
            format: TextureFormat::Rgba8Unorm,
            usage: TextureUsages::TEXTURE_BINDING | TextureUsages::STORAGE_BINDING,
            view_formats: &[],
        });

        Self {
            device,
            queue,
            graph_input: DynamicBuffer::default().with_usage(BufferUsages::STORAGE),

            initialized: None,
            main_bind_group: None,
            external_var_buffers: Vec::new(),
            tile_info: BufferVec::default().with_usage(BufferUsages::STORAGE),
            empty_texture: GpuImage {
                texture: empty_texture,
            },
        }
    }

    pub fn initialize(
        &mut self,
        brush: &mut BrushPresetInstance,
        assets: &AssetRegistry,
        target_layer_texel: TexelType,
    ) {
        let estimated_size = brush.estimate_size();

        let estimated_tile_count = GpuTileStorageInner::calc_tile_count(brush.estimate_size()) + 2;
        let output_len = estimated_tile_count.element_product();
        // TODO: Handle shader compile error
        let (shader, texture_usage_recorder) = brush.compile().unwrap();

        // Prepare referenced textures

        let mut textures = Vec::new();
        for id in texture_usage_recorder.get_usage().keys() {
            if id == &TextureId::NULL {
                textures.push(self.empty_texture.texture.create_view(&Default::default()));
                continue;
            }

            let handle = assets.handle(AssetId::new(**id)).unwrap();
            let gpu_image = GpuImage::from_asset(
                &self.device,
                &self.queue,
                &handle.get().unwrap(),
                TextureUsages::TEXTURE_BINDING,
            );
            textures.push(gpu_image.texture.create_view(&Default::default()));
        }

        // Prepare bind group layout

        let mut bind_group_entries = vec![
            // Graph Input Parameters
            BindGroupLayoutEntry {
                binding: 0,
                visibility: ShaderStages::COMPUTE,
                ty: BindingType::Buffer {
                    ty: BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: Some(GraphInputUniform::min_size()),
                },
                count: None,
            },
            // Output
            BindGroupLayoutEntry {
                binding: 1,
                visibility: ShaderStages::COMPUTE,
                ty: BindingType::StorageTexture {
                    access: StorageTextureAccess::ReadWrite,
                    // TODO: This should be selected by user. If they want to use 16bit textures, this should be rgba16, and convert
                    //       into target color space when merging down.
                    format: target_layer_texel.wgpu_format(),
                    view_dimension: TextureViewDimension::D2,
                },
                count: Some(NonZeroU32::new(output_len).unwrap()),
            },
            // Textures
            BindGroupLayoutEntry {
                binding: 2,
                visibility: ShaderStages::COMPUTE,
                ty: BindingType::Texture {
                    sample_type: TextureSampleType::Float { filterable: true },
                    view_dimension: TextureViewDimension::D2,
                    multisampled: false,
                },
                count: Some(
                    NonZeroU32::new(texture_usage_recorder.get_usage().len() as u32).unwrap(),
                ),
            },
            // Tile Info
            BindGroupLayoutEntry {
                binding: 3,
                visibility: ShaderStages::COMPUTE,
                ty: BindingType::Buffer {
                    ty: BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: Some(TileInfo::min_size()),
                },
                count: None,
            },
        ];

        let mut external_variable_bindings = String::new();
        for var in brush.external_vars().all().values() {
            let cur_binding = bind_group_entries.len() as u32;
            external_variable_bindings
                .extend(generate_external_variable_binding(0, cur_binding, var.as_ref()).chars());
            bind_group_entries.push(BindGroupLayoutEntry {
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
        let shader = shader.replace(
            "//CODEGENFLAG_EXTERNAL_VARIABLE_BINDINGS",
            &external_variable_bindings,
        );
        let shader = compile_brush_wesl(shader).unwrap();
        println!("Generated shader:\n{}", shader);

        let main_layout = self
            .device
            .create_bind_group_layout(&BindGroupLayoutDescriptor {
                label: Some("brush main layout"),
                entries: &bind_group_entries,
            });

        let pipeline_layout = self
            .device
            .create_pipeline_layout(&PipelineLayoutDescriptor {
                label: Some("brush pipeline layout"),
                bind_group_layouts: &[&main_layout],
                push_constant_ranges: &[],
            });

        let shader = self.device.create_shader_module(ShaderModuleDescriptor {
            label: Some("brush shader"),
            source: ShaderSource::Wgsl(shader.into()),
        });

        let pipeline = self
            .device
            .create_compute_pipeline(&ComputePipelineDescriptor {
                label: Some("brush pipeline"),
                layout: Some(&pipeline_layout),
                module: &shader,
                entry_point: Some("main"),
                compilation_options: Default::default(),
                cache: None,
            });

        self.initialized = Some(InitializedData {
            estimated_size,
            output_len_in_layout: output_len,
            pipeline,
            main_layout,
            target_layer_texel,
            textures,
        });
    }

    pub fn prepare(
        &mut self,
        brush: &mut BrushPresetInstance,
        params: GraphInputParams,
        output_layer: LayerId,
        tiles: &GpuTileStorage,
    ) {
        let Some(initialized) = self.initialized.as_ref() else {
            return;
        };

        let estimated_area = brush.estimate_area(&params);
        self.graph_input.clear();
        self.graph_input.push(&GraphInputUniform {
            shader_origin: estimated_area.min,
            estimated_brush_size: initialized.estimated_size,
            tile_size: GpuTileStorageInner::TILE_SIZE,
            pen_position: params.pen_position,
        });
        self.graph_input.write_buffer(&self.device);

        let outputs = tiles.get_tiles_mut_ordered(output_layer, estimated_area);
        self.tile_info.clear();
        for tile in &outputs {
            self.tile_info.push(&TileInfo {
                tile_origin: tile.index.coord * GpuTileStorageInner::TILE_SIZE as i32,
            });
        }
        self.tile_info.write_buffer(&self.device);

        // Prepare output tiles

        let mut empty_placeholders = Vec::new();
        let mut outputs = outputs.iter().map(|t| t.view.as_ref()).collect::<Vec<_>>();
        if outputs.len() < initialized.output_len_in_layout as usize {
            for _ in outputs.len()..initialized.output_len_in_layout as usize {
                let view = self
                    .device
                    .create_texture(&TextureDescriptor {
                        label: Some("empty placeholder texture"),
                        size: Extent3d {
                            width: 1,
                            height: 1,
                            depth_or_array_layers: 1,
                        },
                        mip_level_count: 1,
                        sample_count: 1,
                        dimension: TextureDimension::D2,
                        format: initialized.target_layer_texel.wgpu_format(),
                        usage: TextureUsages::TEXTURE_BINDING | TextureUsages::STORAGE_BINDING,
                        view_formats: &[],
                    })
                    .create_view(&Default::default());
                empty_placeholders.push(view);
            }
            outputs.extend(&empty_placeholders);
        }

        let texture_views = initialized
            .textures
            .iter()
            .map(std::convert::identity)
            .collect::<Vec<_>>();

        let mut bind_group_entries = vec![
            BindGroupEntry {
                binding: 0,
                resource: self.graph_input.binding().unwrap(),
            },
            BindGroupEntry {
                binding: 1,
                resource: BindingResource::TextureViewArray(&outputs),
            },
            BindGroupEntry {
                binding: 2,
                resource: BindingResource::TextureViewArray(&texture_views),
            },
            BindGroupEntry {
                binding: 3,
                resource: self.tile_info.binding().unwrap(),
            },
        ];

        // Prepare external variable buffers
        self.external_var_buffers.clear();
        for var in brush.external_vars().all().values() {
            let buffer = var.value.try_write_into_shader_buffer().unwrap();
            let gpu_buffer = self.device.create_buffer_init(&BufferInitDescriptor {
                label: Some("external variable buffer"),
                contents: &buffer,
                usage: BufferUsages::STORAGE,
            });
            self.external_var_buffers.push(gpu_buffer);
        }
        for buffer in self.external_var_buffers.iter() {
            bind_group_entries.push(BindGroupEntry {
                binding: bind_group_entries.len() as u32,
                resource: buffer.as_entire_binding(),
            });
        }

        let main_bind_group = self.device.create_bind_group(&BindGroupDescriptor {
            label: Some("brush main bind group"),
            layout: &initialized.main_layout,
            entries: &bind_group_entries,
        });
        self.main_bind_group = Some(main_bind_group);
    }

    pub fn draw(&self) {
        let (Some(initialized), Some(main_bind_group)) =
            (self.initialized.as_ref(), self.main_bind_group.as_ref())
        else {
            return;
        };

        let mut ec = self.device.create_command_encoder(&Default::default());

        {
            let mut pass = ec.begin_compute_pass(&Default::default());
            pass.set_pipeline(&initialized.pipeline);
            pass.set_bind_group(0, main_bind_group, &[]);
            pass.dispatch_workgroups(
                initialized.estimated_size.x.div_ceil(16),
                initialized.estimated_size.y.div_ceil(16),
                1,
            );
        }

        self.queue.submit([ec.finish()]);
    }
}
