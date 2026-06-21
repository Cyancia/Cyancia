use std::any::TypeId;

use bevy_math::IRect;
use cyancia_render::bind_group_layout_entries::{BindGroupLayoutEntries, binding_types};
use glam::{IVec2, UVec3};
use wesl::{VirtualResolver, Wesl};
use wgpu::{
    BindGroup, BindGroupDescriptor, BindGroupEntry, BindGroupLayout, BindGroupLayoutDescriptor,
    BindingResource, Buffer, ComputePass, ComputePipeline, ComputePipelineDescriptor, Device,
    PipelineLayoutDescriptor, Queue, ShaderModuleDescriptor, ShaderSource, ShaderStages,
    StorageTextureAccess, TextureView,
};

use crate::{
    CImage,
    composite::{ImageCompositor, LayerPreviewOverriders},
    dynamic_intermediate_buffer::IntermediateBuffer,
    layer::{Layer, LayerData, LayerStackNode},
    tile::{GpuTileInfo, GpuTileStorage},
};

#[derive(Debug, Clone)]
pub struct GroupLayer;

impl Layer for GroupLayer {
    fn can_have_children_of(&self, _: TypeId) -> bool {
        true
    }

    fn can_contain_pixels(&self) -> bool {
        false
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
        for child_node in node.iter_children_composite_order() {
            let child_layer = image.layer_stack().get_layer(child_node.id).unwrap();
            child_layer.create_blend_cache(
                compositor, overriders, image, child_node, tiles, device, queue,
            );
        }

        let tile_rect = GpuTileStorage::pixel_rect_to_tile(IRect {
            min: IVec2::ZERO,
            max: image.size().as_ivec2(),
        });

        if let Some(cache) = compositor.get_blend_cache::<GroupBlendCache>(&layer.id())
            && cache.blend_func_name == layer.blend_func.name()
            && cache.intermediate.texel_type() == image.texel_type()
            && cache.intermediate.tile_rect() == tile_rect
        {
            return;
        }

        let shader = include_str!("../blend_layers.wesl").replace(
            "//CODEGEN_BLEND_FUNC",
            &layer.blend_func.wgsl_function_call("src", "dst"),
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
            entries: &BindGroupLayoutEntries::sequential(
                ShaderStages::COMPUTE,
                (
                    binding_types::texture_storage_2d_array(
                        image.texel_type().wgpu_format(),
                        StorageTextureAccess::ReadOnly,
                    ),
                    binding_types::storage_buffer_read_only::<GpuTileInfo>(false),
                    binding_types::texture_storage_2d_array(
                        image.texel_type().wgpu_format(),
                        StorageTextureAccess::ReadOnly,
                    ),
                    binding_types::storage_buffer_read_only::<GpuTileInfo>(false),
                    binding_types::texture_storage_2d_array(
                        image.texel_type().wgpu_format(),
                        StorageTextureAccess::WriteOnly,
                    ),
                    binding_types::storage_buffer_read_only::<GpuTileInfo>(false),
                ),
            ),
        });

        let pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: "layer blend pipeline layout".into(),
            bind_group_layouts: &[Some(&layout)],
            ..Default::default()
        });

        let pipeline = device.create_compute_pipeline(&ComputePipelineDescriptor {
            label: "layer blend pipeline".into(),
            layout: Some(&pipeline_layout),
            module: &shader_module,
            entry_point: "main".into(),
            compilation_options: Default::default(),
            cache: None,
        });

        let cache = GroupBlendCache {
            blend_func_name: layer.blend_func.name(),
            intermediate: IntermediateBuffer::new(device, queue, tile_rect, image.texel_type()),
            layout,
            pipeline,
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
        let Some(cache) = compositor.get_blend_cache::<GroupBlendCache>(&layer.id()) else {
            log::error!("BlendCache is not created for layer {}", layer.name);
            return;
        };

        let mut next_output = 1;
        let textures = cache.intermediate.textures().clone();
        let tile_info = cache.intermediate.tile_info_buffer().clone();

        for child_node in node.iter_children_composite_order() {
            let child_layer = image.layer_stack().get_layer(child_node.id).unwrap();
            child_layer.prepare_blend_cache(
                compositor,
                overriders,
                image,
                child_node,
                tiles,
                &textures[1 - next_output],
                &tile_info,
                &textures[next_output],
                &tile_info,
                device,
                queue,
            );
            next_output = 1 - next_output;
        }

        let cache = compositor
            .get_blend_cache_mut::<GroupBlendCache>(&layer.id())
            .unwrap();

        let bind_group = device.create_bind_group(&BindGroupDescriptor {
            label: "layer blend bind group".into(),
            layout: &cache.layout,
            entries: &[
                BindGroupEntry {
                    binding: 0,
                    resource: BindingResource::TextureView(
                        &cache.intermediate.textures()[1 - next_output],
                    ),
                },
                BindGroupEntry {
                    binding: 1,
                    resource: cache.intermediate.tile_info_buffer().as_entire_binding(),
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
        for child_node in node.iter_children_composite_order() {
            let child_layer = image.layer_stack().get_layer(child_node.id).unwrap();
            child_layer.dispatch_blend(compositor, pass, image, child_node, tiles);
        }

        let Some(cache) = compositor.get_blend_cache::<GroupBlendCache>(&layer.id()) else {
            log::error!("BlendCache is not created for layer {}", layer.name);
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

pub struct GroupBlendCache {
    blend_func_name: String,
    intermediate: IntermediateBuffer,
    layout: BindGroupLayout,
    pipeline: ComputePipeline,
    dispatch: Option<(BindGroup, UVec3)>,
}
