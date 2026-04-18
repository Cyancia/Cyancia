use std::{
    collections::{HashMap, hash_map::Entry},
    sync::Arc,
};

use bevy_math::IRect;
use dyn_clone::DynClone;
use encase::ShaderType;
use glam::{IVec2, UVec2, UVec3};
use wesl::{VirtualResolver, Wesl};
use wgpu::{
    BindGroup, BindGroupDescriptor, BindGroupEntry, BindGroupLayoutDescriptor, BindGroupLayoutEntry, BindingResource, BindingType, Buffer, BufferBindingType, ComputePass, ComputePassDescriptor, ComputePipeline, ComputePipelineDescriptor, Device, Extent3d, Origin3d, PipelineLayoutDescriptor, Queue, ShaderModuleDescriptor, ShaderSource, ShaderStages, StorageTextureAccess, TexelCopyTextureInfo, TextureView, TextureViewDimension
};

use crate::{
    CImage,
    dynamic_intermediate_buffer::IntermediateBuffer,
    layer::{Layer, LayerId, LayerStackNode},
    texel::TexelType,
    tile::{GpuTileInfo, GpuTileStorage, GpuTileStorageInner},
};

pub trait BlendFunction: Send + Sync + DynClone + 'static {
    fn name(&self) -> String;
    fn wgsl_function_call(&self, src_ident: &str, dst_ident: &str) -> String;
}
dyn_clone::clone_trait_object!(BlendFunction);

pub struct BlendCache {
    pipeline: ComputePipeline,
    dispatch: Option<(BindGroup, UVec3)>,
}

impl BlendCache {
    pub fn new(
        layer: &Layer,
        tiles: &GpuTileStorage,
        output_texel_type: TexelType,
        device: &Device,
    ) -> BlendCache {
        let shader = include_str!("shaders/blend_layers.wesl").replace(
            "//CODEGEN_BLEND_FUNC",
            &layer
                .blend_func
                .wgsl_function_call("src".into(), "dst".into()),
        );

        let mut resolver = VirtualResolver::new();
        resolver.add_module("package::template".parse().unwrap(), shader.into());
        resolver.add_module(
            "package::image::blend_modes".parse().unwrap(),
            include_str!("shaders/blend_modes.wesl").into(),
        );
        resolver.add_module(
            "package::image::image_tilling".parse().unwrap(),
            include_str!("shaders/image_tiling.wesl").into(),
        );
        resolver.add_module(
            "package::image::texture_unpack".parse().unwrap(),
            include_str!("shaders/texture_unpack.wesl").into(),
        );

        let mut compiler = Wesl::new_barebones().set_custom_resolver(resolver);
        compiler.set_mangler(Default::default());
        compiler.set_options(Default::default());
        let compiled_shader = match compiler.compile(&"package::template".parse().unwrap()) {
            Ok(s) => s.to_string(),
            Err(e) => {
                panic!("Failed to compile blend shader: {}", e);
            }
        };

        let shader_module = device.create_shader_module(ShaderModuleDescriptor {
            label: "layer blend shader".into(),
            source: ShaderSource::Wgsl(compiled_shader.into()),
        });

        let layer_info = tiles.get_layer_info(layer.id()).unwrap();
        let layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: "layer blend bind group layout".into(),
            entries: &[
                BindGroupLayoutEntry {
                    binding: 0,
                    visibility: ShaderStages::COMPUTE,
                    ty: BindingType::StorageTexture {
                        access: StorageTextureAccess::ReadOnly,
                        format: layer_info.texel_type.wgpu_format(),
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
                        access: StorageTextureAccess::ReadOnly,
                        format: layer_info.texel_type.wgpu_format(),
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
                    ty: BindingType::StorageTexture {
                        access: StorageTextureAccess::WriteOnly,
                        format: output_texel_type.wgpu_format(),
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
            label: "layer blend pipeline layout".into(),
            bind_group_layouts: &[&layout],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_compute_pipeline(&ComputePipelineDescriptor {
            label: "layer blend pipeline".into(),
            layout: Some(&pipeline_layout),
            module: &shader_module,
            entry_point: "main".into(),
            compilation_options: Default::default(),
            cache: None,
        });

        Self {
            pipeline,
            dispatch: None,
        }
    }

    pub fn prepare(
        &mut self,
        image_size: UVec2,
        src_buffer: &TextureView,
        src_tile_info: &Buffer,
        dst_buffer: &TextureView,
        dst_tile_info: &Buffer,
        output: &TextureView,
        output_tile_info: &Buffer,
        device: &Device,
    ) {
        let bind_group = device.create_bind_group(&BindGroupDescriptor {
            label: "layer blend bind group".into(),
            layout: &self.pipeline.get_bind_group_layout(0),
            entries: &[
                BindGroupEntry {
                    binding: 0,
                    resource: BindingResource::TextureView(src_buffer),
                },
                BindGroupEntry {
                    binding: 1,
                    resource: src_tile_info.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 2,
                    resource: BindingResource::TextureView(dst_buffer),
                },
                BindGroupEntry {
                    binding: 3,
                    resource: dst_tile_info.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 4,
                    resource: BindingResource::TextureView(output),
                },
                BindGroupEntry {
                    binding: 5,
                    resource: output_tile_info.as_entire_binding(),
                },
            ],
        });

        let workgroup_count = UVec3::new(image_size.x.div_ceil(16), image_size.y.div_ceil(16), 1);

        self.dispatch = Some((bind_group, workgroup_count));
    }

    pub fn dispatch(&self, pass: &mut ComputePass) {
        let Some((bind_group, workgroup_count)) = &self.dispatch else {
            log::error!("BlendCache bind group is not prepared");
            return;
        };

        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, bind_group, &[]);
        pass.dispatch_workgroups(workgroup_count.x, workgroup_count.y, workgroup_count.z);
    }
}

pub struct ImageCompositor {
    cache: HashMap<LayerId, BlendCache>,
}

impl ImageCompositor {
    pub fn new() -> Self {
        Self {
            cache: HashMap::new(),
        }
    }

    // TODO: incremental cache building
    pub fn build_cache(&mut self, image: &CImage, tiles: &GpuTileStorage, device: &Device) {
        self.cache.clear();
        for layer in image.layers.iter_layers_dfs_without_root() {
            let cache = BlendCache::new(layer, tiles, TexelType::RGBA8, device);
            self.cache.insert(layer.id(), cache);
        }
    }

    pub fn composite(
        &mut self,
        image: &CImage,
        tiles: &GpuTileStorage,
        device: &Device,
        queue: &Queue,
    ) {
        let mut ec = device.create_command_encoder(&Default::default());
        let mut pass = ec.begin_compute_pass(&ComputePassDescriptor {
            label: Some("image composite pass"),
            ..Default::default()
        });

        let tile_rect = GpuTileStorageInner::pixel_rect_to_tile(IRect {
            min: IVec2::ZERO,
            max: image.size.as_ivec2(),
        });
        let intermediate = IntermediateBuffer::new(device, queue, tile_rect, TexelType::RGBA8);
        let root_node = image.layer_stack().root_node();

        let mut next_output = 1;
        for child_node in root_node.children() {
            let child_layer = image.layer_stack().get_layer(child_node.id()).unwrap();
            blend_onto(
                child_layer,
                &intermediate.textures()[1 - next_output],
                intermediate.tile_info_buffer(),
                &intermediate.textures()[next_output],
                intermediate.tile_info_buffer(),
                &mut pass,
                image,
                tiles,
                child_node,
                &mut self.cache,
                device,
                queue,
            );
            next_output = 1 - next_output;
        }

        drop(pass);

        let mut root = tiles.get_layer_mut(root_node.id()).unwrap();
        root.ensure_tile_area(tile_rect);
        for y in tile_rect.min.y..tile_rect.max.y {
            for x in tile_rect.min.x..tile_rect.max.x {
                let coord = IVec2 { x, y };
                let z_src = intermediate.coord_to_layer(coord).unwrap();
                let z_dst = root.get_tile_layer(coord).unwrap();
                ec.copy_texture_to_texture(
                    TexelCopyTextureInfo {
                        texture: intermediate.textures()[1 - next_output].texture(),
                        mip_level: 0,
                        origin: Origin3d {
                            z: z_src,
                            ..Default::default()
                        },
                        aspect: Default::default(),
                    },
                    TexelCopyTextureInfo {
                        texture: root.texture().unwrap().texture(),
                        mip_level: 0,
                        origin: Origin3d {
                            z: z_dst,
                            ..Default::default()
                        },
                        aspect: Default::default(),
                    },
                    Extent3d {
                        width: GpuTileStorageInner::TILE_SIZE,
                        height: GpuTileStorageInner::TILE_SIZE,
                        depth_or_array_layers: 1,
                    },
                );
            }
        }

        queue.submit([ec.finish()]);
    }
}

fn blend_onto(
    src_layer: &Layer,
    dst_buffer: &TextureView,
    dst_tile_info: &Buffer,
    output_buffer: &TextureView,
    output_tile_info: &Buffer,
    pass: &mut ComputePass,
    image: &CImage,
    tiles: &GpuTileStorage,
    layer_node: &LayerStackNode,
    cache: &mut HashMap<LayerId, BlendCache>,
    device: &Device,
    queue: &Queue,
) {
    if layer_node.children().is_empty() {
        let cache = cache.get_mut(&src_layer.id()).unwrap();
        let layer_binding = tiles.get_layer_binding_or_empty(src_layer.id()).unwrap();
        cache.prepare(
            image.size,
            &layer_binding.texture,
            &layer_binding.tile_info_buffer,
            dst_buffer,
            dst_tile_info,
            output_buffer,
            output_tile_info,
            device,
        );
        cache.dispatch(pass);
    } else {
        let tile_rect = GpuTileStorageInner::pixel_rect_to_tile(IRect {
            min: IVec2::ZERO,
            max: image.size.as_ivec2(),
        });
        let intermediate = IntermediateBuffer::new(device, queue, tile_rect, TexelType::RGBA8);

        let mut next_output = 1;
        for child_node in layer_node.children() {
            let child_layer = image.layer_stack().get_layer(child_node.id()).unwrap();
            blend_onto(
                child_layer,
                &intermediate.textures()[1 - next_output],
                intermediate.tile_info_buffer(),
                &intermediate.textures()[next_output],
                intermediate.tile_info_buffer(),
                pass,
                image,
                tiles,
                child_node,
                cache,
                device,
                queue,
            );
            next_output = 1 - next_output;
        }

        let cache = cache.get_mut(&src_layer.id()).unwrap();
        cache.prepare(
            image.size,
            &intermediate.textures()[1 - next_output],
            intermediate.tile_info_buffer(),
            dst_buffer,
            dst_tile_info,
            output_buffer,
            output_tile_info,
            device,
        );
        cache.dispatch(pass);
    }
}
