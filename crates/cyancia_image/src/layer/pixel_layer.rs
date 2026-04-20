use std::any::TypeId;

use encase::ShaderType;
use glam::{IVec2, UVec2, UVec3};
use wesl::{VirtualResolver, Wesl};
use wgpu::{
    BindGroup, BindGroupDescriptor, BindGroupEntry, BindGroupLayout, BindGroupLayoutDescriptor,
    BindGroupLayoutEntry, BindingResource, BindingType, Buffer, BufferBindingType, ComputePass,
    ComputePassDescriptor, ComputePipeline, ComputePipelineDescriptor, Device, Extent3d, Origin3d,
    PipelineLayoutDescriptor, Queue, ShaderModuleDescriptor, ShaderSource, ShaderStages,
    StorageTextureAccess, TexelCopyTextureInfo, TextureView, TextureViewDimension,
};

use crate::{
    CImage,
    composite::ImageCompositor,
    layer::{Layer, LayerData, LayerStackNode},
    texel::TexelType,
    tile::{GpuTileInfo, GpuTileStorage},
};

#[derive(Debug, Clone)]
pub struct PixelLayer;

impl Layer for PixelLayer {
    fn can_have_children_of(&self, ty: TypeId) -> bool {
        false
    }

    fn can_contain_pixels(&self) -> bool {
        true
    }

    fn create_blend_cache(
        &self,
        compositor: &mut ImageCompositor,
        image: &CImage,
        layer: &LayerData,
        node: &LayerStackNode,
        tiles: &GpuTileStorage,
        device: &Device,
        queue: &Queue,
    ) {
        let layer_info = tiles.get_layer_info(layer.id()).unwrap();

        if let Some(cache) = compositor.get_blend_cache::<PixelBlendCache>(&layer.id()) {
            if cache.blend_func_name == layer.blend_func.name()
                && cache.layer_texel_type == layer_info.texel_type
                && cache.image_texel_type == image.texel_type()
            {
                return;
            }
        }

        let shader = include_str!("../shaders/blend_layers.wesl").replace(
            "//CODEGEN_BLEND_FUNC",
            &layer
                .blend_func
                .wgsl_function_call("src".into(), "dst".into()),
        );

        let mut resolver = VirtualResolver::new();
        resolver.add_module("package::template".parse().unwrap(), shader.into());
        resolver.add_module(
            "package::image::blend_modes".parse().unwrap(),
            include_str!("../shaders/blend_modes.wesl").into(),
        );
        resolver.add_module(
            "package::image::image_tilling".parse().unwrap(),
            include_str!("../shaders/image_tiling.wesl").into(),
        );
        resolver.add_module(
            "package::image::texture_unpack".parse().unwrap(),
            include_str!("../shaders/texture_unpack.wesl").into(),
        );

        let mut compiler = Wesl::new_barebones().set_custom_resolver(resolver);
        compiler.set_mangler(Default::default());
        compiler.set_options(Default::default());
        let compiled_shader = match compiler.compile(&"package::template".parse().unwrap()) {
            Ok(s) => s.to_string(),
            Err(e) => {
                // TODO: Don't panic.
                panic!("Failed to compile blend shader: {}", e);
            }
        };

        let shader_module = device.create_shader_module(ShaderModuleDescriptor {
            label: "layer blend shader".into(),
            source: ShaderSource::Wgsl(compiled_shader.into()),
        });

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
                        format: image.texel_type().wgpu_format(),
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

        let cache = PixelBlendCache {
            blend_func_name: layer.blend_func.name(),
            layer_texel_type: layer_info.texel_type,
            image_texel_type: image.texel_type(),
            layout,
            pipeline,
            dispatch: None,
        };
        compositor.insert_blend_cache(layer.id(), cache);
    }

    fn prepare_blend_cache(
        &self,
        compositor: &mut ImageCompositor,
        image: &CImage,
        layer: &LayerData,
        node: &LayerStackNode,
        tiles: &GpuTileStorage,
        dst_buffer: &TextureView,
        dst_tile_info: &Buffer,
        output: &TextureView,
        output_tile_info: &Buffer,
        device: &Device,
        queue: &Queue,
    ) {
        let src = tiles.get_layer_binding_or_empty(layer.id).unwrap();
        let Some(cache) = compositor.get_blend_cache_mut::<PixelBlendCache>(&layer.id()) else {
            log::error!("BlendCache is not created for layer {:?}", layer.id());
            return;
        };

        let bind_group = device.create_bind_group(&BindGroupDescriptor {
            label: "layer blend bind group".into(),
            layout: &cache.layout,
            entries: &[
                BindGroupEntry {
                    binding: 0,
                    resource: BindingResource::TextureView(&src.texture),
                },
                BindGroupEntry {
                    binding: 1,
                    resource: src.tile_info_buffer.as_entire_binding(),
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

        let workgroup_count =
            UVec3::new(image.size().x.div_ceil(16), image.size().y.div_ceil(16), 1);

        cache.dispatch = Some((bind_group, workgroup_count));
    }

    fn dispatch_blend(
        &self,
        compositor: &ImageCompositor,
        pass: &mut ComputePass,
        image: &CImage,
        layer: &LayerData,
        node: &LayerStackNode,
        tiles: &GpuTileStorage,
    ) {
        let Some(cache) = compositor.get_blend_cache::<PixelBlendCache>(&layer.id()) else {
            log::error!("BlendCache is not created for layer {:?}", layer.id());
            return;
        };

        let Some((bind_group, workgroup_count)) = &cache.dispatch else {
            log::error!("BlendCache bind group is not prepared");
            return;
        };

        pass.set_pipeline(&cache.pipeline);
        pass.set_bind_group(0, bind_group, &[]);
        pass.dispatch_workgroups(workgroup_count.x, workgroup_count.y, workgroup_count.z);
    }
}

pub struct PixelBlendCache {
    blend_func_name: String,
    layer_texel_type: TexelType,
    image_texel_type: TexelType,
    layout: BindGroupLayout,
    pipeline: ComputePipeline,
    dispatch: Option<(BindGroup, UVec3)>,
}
