use std::any::TypeId;

use encase::ShaderType;
use glam::{IVec2, UVec2, UVec3};
use wesl::{VirtualResolver, Wesl};
use wgpu::{
    BindGroup, BindGroupDescriptor, BindGroupEntry, BindGroupLayout, BindGroupLayoutDescriptor,
    BindGroupLayoutEntry, BindingResource, BindingType, Buffer, BufferBindingType, ComputePass,
    ComputePassDescriptor, ComputePipeline, ComputePipelineDescriptor, Device, Extent3d, Origin3d,
    PipelineLayoutDescriptor, Queue, ShaderModuleDescriptor, ShaderSource, ShaderStages,
    StorageTextureAccess, TexelCopyTextureInfo, TextureDescriptor, TextureDimension, TextureUsages,
    TextureView, TextureViewDimension, wgt::TextureViewDescriptor,
};

use crate::{
    CImage,
    composite::{ImageCompositor, LayerPreviewOverriders, PixelPreviewOverrider},
    dynamic_intermediate_buffer::DynamicGpuTileInfoBuffer,
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
        overriders: &mut LayerPreviewOverriders,
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

        let shader = include_str!("../blend_layers.wesl").replace(
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
        let without_overrider_shader = match compiler.compile(&"package::template".parse().unwrap())
        {
            Ok(s) => s.to_string(),
            Err(e) => {
                // TODO: Don't panic.
                panic!("Failed to compile blend shader: {}", e);
            }
        };

        compiler.set_feature("OVERRIDER", true);
        let with_overrider_shader = match compiler.compile(&"package::template".parse().unwrap()) {
            Ok(s) => s.to_string(),
            Err(e) => {
                // TODO: Don't panic.
                panic!("Failed to compile blend shader: {}", e);
            }
        };

        let with_overrider_shader_module = device.create_shader_module(ShaderModuleDescriptor {
            label: "pixel layer blend shader".into(),
            source: ShaderSource::Wgsl(with_overrider_shader.into()),
        });
        let without_overrider_shader_module = device.create_shader_module(ShaderModuleDescriptor {
            label: "pixel layer blend shader".into(),
            source: ShaderSource::Wgsl(without_overrider_shader.into()),
        });

        let mut entries = vec![
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
        ];
        let without_overrider_layout =
            device.create_bind_group_layout(&BindGroupLayoutDescriptor {
                label: "pixel layer blend bind group layout without overrider".into(),
                entries: &entries,
            });

        entries.extend([
            BindGroupLayoutEntry {
                binding: 6,
                visibility: ShaderStages::COMPUTE,
                ty: BindingType::StorageTexture {
                    access: StorageTextureAccess::ReadOnly,
                    format: image.texel_type().wgpu_format(),
                    view_dimension: TextureViewDimension::D2Array,
                },
                count: None,
            },
            BindGroupLayoutEntry {
                binding: 7,
                visibility: ShaderStages::COMPUTE,
                ty: BindingType::Buffer {
                    ty: BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: Some(GpuTileInfo::min_size()),
                },
                count: None,
            },
        ]);
        let with_overrider_layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: "pixel layer blend bind group layout with overrider".into(),
            entries: &entries,
        });

        let with_overrider_pipeline_layout =
            device.create_pipeline_layout(&PipelineLayoutDescriptor {
                label: "pixel layer blend pipeline layout with overrider".into(),
                bind_group_layouts: &[&with_overrider_layout],
                push_constant_ranges: &[],
            });

        let without_overrider_pipeline_layout =
            device.create_pipeline_layout(&PipelineLayoutDescriptor {
                label: "pixel layer blend pipeline layout without overrider".into(),
                bind_group_layouts: &[&without_overrider_layout],
                push_constant_ranges: &[],
            });

        let with_overrider_pipeline = device.create_compute_pipeline(&ComputePipelineDescriptor {
            label: "pixel layer blend pipeline with overrider".into(),
            layout: Some(&with_overrider_pipeline_layout),
            module: &with_overrider_shader_module,
            entry_point: "main".into(),
            compilation_options: Default::default(),
            cache: None,
        });

        let without_overrider_pipeline =
            device.create_compute_pipeline(&ComputePipelineDescriptor {
                label: "pixel layer blend pipeline without overrider".into(),
                layout: Some(&without_overrider_pipeline_layout),
                module: &without_overrider_shader_module,
                entry_point: "main".into(),
                compilation_options: Default::default(),
                cache: None,
            });

        let cache = PixelBlendCache {
            blend_func_name: layer.blend_func.name(),
            layer_texel_type: layer_info.texel_type,
            image_texel_type: image.texel_type(),
            with_overrider_pipeline,
            without_overrider_pipeline,
            with_overrider_layout,
            without_overrider_layout,
            dispatch: None,
        };
        compositor.insert_blend_cache(layer.id(), cache);
    }

    fn prepare_blend_cache(
        &self,
        compositor: &mut ImageCompositor,
        overriders: &LayerPreviewOverriders,
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

        let (bind_group, pipeline) = if let Some(overrider) =
            overriders.get_overrider::<PixelPreviewOverrider>(&layer.id())
        {
            let bg = device.create_bind_group(&BindGroupDescriptor {
                label: "pixel layer blend bind group with overrider".into(),
                layout: &cache.with_overrider_layout,
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
                    BindGroupEntry {
                        binding: 6,
                        resource: BindingResource::TextureView(&overrider.texture),
                    },
                    BindGroupEntry {
                        binding: 7,
                        resource: overrider.tile_info_buffer.as_entire_binding(),
                    },
                ],
            });

            (bg, cache.with_overrider_pipeline.clone())
        } else {
            let bg = device.create_bind_group(&BindGroupDescriptor {
                label: "pixel layer blend bind group without overrider".into(),
                layout: &cache.without_overrider_layout,
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

            (bg, cache.without_overrider_pipeline.clone())
        };

        let workgroup_count =
            UVec3::new(image.size().x.div_ceil(16), image.size().y.div_ceil(16), 1);

        cache.dispatch = Some((pipeline, bind_group, workgroup_count));
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

        let Some((pipeline, bind_group, workgroup_count)) = &cache.dispatch else {
            log::error!("BlendCache bind group is not prepared");
            return;
        };

        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, bind_group, &[]);
        pass.dispatch_workgroups(workgroup_count.x, workgroup_count.y, workgroup_count.z);
    }
}

pub struct PixelBlendCache {
    blend_func_name: String,
    layer_texel_type: TexelType,
    image_texel_type: TexelType,
    with_overrider_pipeline: ComputePipeline,
    without_overrider_pipeline: ComputePipeline,
    with_overrider_layout: BindGroupLayout,
    without_overrider_layout: BindGroupLayout,
    dispatch: Option<(ComputePipeline, BindGroup, UVec3)>,
}
