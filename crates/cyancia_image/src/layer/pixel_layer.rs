use std::any::TypeId;

use cyancia_render::{
    bind_group_entries::DynamicBindGroupEntries,
    bind_group_layout_entries::{
        BindGroupLayoutEntries, DynamicBindGroupLayoutEntries, binding_types,
    },
    buffer::DynamicBuffer,
};
use glam::UVec3;
use wesl::{VirtualResolver, Wesl};
use wgpu::{
    BindGroup, BindGroupDescriptor, BindGroupLayoutDescriptor, Buffer, BufferUsages, ComputePass,
    ComputePipeline, ComputePipelineDescriptor, Device, PipelineLayoutDescriptor, Queue,
    ShaderModuleDescriptor, ShaderSource, ShaderStages, StorageTextureAccess, TextureView,
};

use crate::{
    CImage,
    composite::{
        BlendFunctionId, BlendFunctionRegistry, BlendLayerParams, ImageCompositor,
        LayerPreviewOverriders, PixelPreviewOverrider,
    },
    layer::{Layer, LayerId},
    texel::TexelType,
    tile::{GpuTileInfo, GpuTileStorage},
};

#[derive(Debug, Clone)]
pub struct PixelLayer;

impl Layer for PixelLayer {
    fn can_have_children_of(&self, _: TypeId) -> bool {
        false
    }

    fn can_contain_pixels(&self) -> bool {
        true
    }

    fn create_blend_cache(
        &self,
        compositor: &mut ImageCompositor,
        _: &mut LayerPreviewOverriders,
        image: &CImage,
        layer_id: LayerId,
        tiles: &GpuTileStorage,
        blend_funcs: &BlendFunctionRegistry,
        device: &Device,
        _: &Queue,
    ) {
        let layer_info = tiles.get_layer_info(layer_id).unwrap();

        let node = image.layer_stack().get_layer(&layer_id).unwrap();

        if let Some(cache) = compositor.get_blend_cache_mut::<PixelBlendCache>(&layer_id)
            && cache.blend_func_name == node.data().blend_func
            && cache.layer_texel_type == layer_info.texel_type
            && cache.image_texel_type == image.texel_type()
        {
            return;
        }

        let blend_func = blend_funcs
            .get(&node.data().blend_func)
            .unwrap_or_else(|| panic!("Blend function {} not found", node.data().blend_func));
        let shader = include_str!("../blend_layers.wesl").replace(
            "//CODEGEN_BLEND_FUNC",
            &blend_func.wgsl_function_call("src", "dst"),
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

        let mut entries = DynamicBindGroupLayoutEntries::sequential(
            ShaderStages::COMPUTE,
            (
                binding_types::uniform_buffer::<BlendLayerParams>(false),
                binding_types::texture_storage_2d_array(
                    layer_info.texel_type.wgpu_format(),
                    StorageTextureAccess::ReadOnly,
                ),
                binding_types::storage_buffer_read_only::<GpuTileInfo>(false),
                binding_types::texture_storage_2d_array(
                    layer_info.texel_type.wgpu_format(),
                    StorageTextureAccess::ReadOnly,
                ),
                binding_types::storage_buffer_read_only::<GpuTileInfo>(false),
                binding_types::texture_storage_2d_array(
                    image.texel_type().wgpu_format(),
                    StorageTextureAccess::WriteOnly,
                ),
                binding_types::storage_buffer_read_only::<GpuTileInfo>(false),
            ),
        )
        .to_vec();
        let without_overrider_layout =
            device.create_bind_group_layout(&BindGroupLayoutDescriptor {
                label: "pixel layer blend bind group layout without overrider".into(),
                entries: &entries,
            });

        entries.extend(
            BindGroupLayoutEntries::with_indices(
                ShaderStages::COMPUTE,
                (
                    (
                        7,
                        binding_types::texture_storage_2d_array(
                            image.texel_type().wgpu_format(),
                            StorageTextureAccess::ReadOnly,
                        ),
                    ),
                    (
                        8,
                        binding_types::storage_buffer_read_only::<GpuTileInfo>(false),
                    ),
                ),
            )
            .as_ref(),
        );
        let with_overrider_layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: "pixel layer blend bind group layout with overrider".into(),
            entries: &entries,
        });

        let with_overrider_pipeline_layout =
            device.create_pipeline_layout(&PipelineLayoutDescriptor {
                label: "pixel layer blend pipeline layout with overrider".into(),
                bind_group_layouts: &[Some(&with_overrider_layout)],
                ..Default::default()
            });

        let without_overrider_pipeline_layout =
            device.create_pipeline_layout(&PipelineLayoutDescriptor {
                label: "pixel layer blend pipeline layout without overrider".into(),
                bind_group_layouts: &[Some(&without_overrider_layout)],
                ..Default::default()
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

        let params_buffer = DynamicBuffer::new(
            Some("pixel layer blend params buffer".into()),
            BufferUsages::UNIFORM,
        );

        let cache = PixelBlendCache {
            blend_func_name: node.data().blend_func.clone(),
            layer_texel_type: layer_info.texel_type,
            image_texel_type: image.texel_type(),
            params_buffer,
            with_overrider_pipeline,
            without_overrider_pipeline,
            dispatch: None,
        };
        compositor.insert_blend_cache(layer_id, cache);
    }

    fn prepare_blend_cache(
        &self,
        compositor: &mut ImageCompositor,
        overriders: &LayerPreviewOverriders,
        image: &CImage,
        layer_id: LayerId,
        tiles: &GpuTileStorage,
        dst_buffer: &TextureView,
        dst_tile_info: &Buffer,
        output: &TextureView,
        output_tile_info: &Buffer,
        device: &Device,
        queue: &Queue,
    ) {
        let src = tiles.get_layer_binding_or_empty(layer_id).unwrap();
        let Some(cache) = compositor.get_blend_cache_mut::<PixelBlendCache>(&layer_id) else {
            log::error!("BlendCache is not created for layer {:?}", layer_id);
            return;
        };
        let node = image.layer_stack().get_layer(&layer_id).unwrap();

        dbg!(node.data().opacity);
        cache.params_buffer.clear();
        cache.params_buffer.push(&BlendLayerParams {
            src_opacity: node.data().opacity,
            src_disabled_channels: node.data().disabled_channels,
            _pad: Default::default(),
        });
        cache.params_buffer.write_buffer(device, queue);

        let mut entries = DynamicBindGroupEntries::sequential((
            cache.params_buffer.binding().unwrap(),
            &src.texture,
            src.tile_info_buffer.as_entire_binding(),
            dst_buffer,
            dst_tile_info.as_entire_binding(),
            output,
            output_tile_info.as_entire_binding(),
        ));

        let pipeline =
            if let Some(overrider) = overriders.get_overrider::<PixelPreviewOverrider>(&layer_id) {
                entries = entries.extend_sequential((
                    &overrider.texture,
                    overrider.tile_info_buffer.as_entire_binding(),
                ));

                cache.with_overrider_pipeline.clone()
            } else {
                cache.without_overrider_pipeline.clone()
            };

        let bind_group = device.create_bind_group(&BindGroupDescriptor {
            label: "pixel layer blend bind group".into(),
            layout: &pipeline.get_bind_group_layout(0),
            entries: &entries,
        });

        let workgroup_count =
            UVec3::new(image.size().x.div_ceil(16), image.size().y.div_ceil(16), 1);

        cache.dispatch = Some((pipeline, bind_group, workgroup_count));
    }

    fn dispatch_blend(
        &self,
        compositor: &ImageCompositor,
        pass: &mut ComputePass,
        _: &CImage,
        layer_id: LayerId,
        _: &GpuTileStorage,
    ) {
        let Some(cache) = compositor.get_blend_cache::<PixelBlendCache>(&layer_id) else {
            log::error!("BlendCache is not created for layer {:?}", layer_id);
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
    blend_func_name: BlendFunctionId,
    layer_texel_type: TexelType,
    image_texel_type: TexelType,
    params_buffer: DynamicBuffer<BlendLayerParams>,
    with_overrider_pipeline: ComputePipeline,
    without_overrider_pipeline: ComputePipeline,
    dispatch: Option<(ComputePipeline, BindGroup, UVec3)>,
}
