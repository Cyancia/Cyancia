use std::{
    any::Any,
    collections::{HashMap, hash_map::Entry},
    sync::Arc,
};

use bevy_math::IRect;
use dyn_clone::DynClone;
use encase::ShaderType;
use glam::{IVec2, UVec2, UVec3};
use gpui::Global;
use wesl::{VirtualResolver, Wesl};
use wgpu::{
    BindGroup, BindGroupDescriptor, BindGroupEntry, BindGroupLayoutDescriptor,
    BindGroupLayoutEntry, BindingResource, BindingType, Buffer, BufferBindingType, ComputePass,
    ComputePassDescriptor, ComputePipeline, ComputePipelineDescriptor, Device, Extent3d, Origin3d,
    PipelineLayoutDescriptor, Queue, ShaderModuleDescriptor, ShaderSource, ShaderStages,
    StorageTextureAccess, TexelCopyTextureInfo, TextureView, TextureViewDimension,
};

use crate::{
    CImage,
    dynamic_intermediate_buffer::IntermediateBuffer,
    layer::{LayerData, LayerId, LayerStackNode},
    texel::TexelType,
    tile::{GpuTileInfo, GpuTileStorage, GpuTileStorageInner},
};

pub trait BlendFunction: Send + Sync + DynClone + 'static {
    fn name(&self) -> String;
    fn wgsl_function_call(&self, src_ident: &str, dst_ident: &str) -> String;
}
dyn_clone::clone_trait_object!(BlendFunction);

#[derive(Default)]
pub struct ImageCompositor {
    cache: HashMap<LayerId, Box<dyn Any + Send + Sync>>,
}

impl ImageCompositor {
    pub fn new() -> Self {
        Self {
            cache: HashMap::new(),
        }
    }

    // TODO: incremental cache building and preparing
    pub fn create_cache(
        &mut self,
        overriders: &mut LayerPreviewOverriders,
        image: &CImage,
        tiles: &GpuTileStorage,
        device: &Device,
        queue: &Queue,
    ) {
        let now = std::time::Instant::now();
        let root_data = image.layer_stack().get_layer(image.root_id()).unwrap();
        root_data.create_blend_cache(
            self,
            overriders,
            image,
            image.layer_stack().root_node(),
            tiles,
            device,
            queue,
        );
        log::debug!("Blend cache created in {:?}", now.elapsed());
    }

    pub fn composite(
        &mut self,
        overriders: &LayerPreviewOverriders,
        dirty_tiles: IRect,
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

        let mut root_layer_tiles = tiles.get_layer_mut(image.root_id()).unwrap();
        root_layer_tiles.ensure_pixel_area(IRect {
            min: IVec2::ZERO,
            max: image.size.as_ivec2(),
        });
        let root_layer_binding = root_layer_tiles.binding_data().unwrap();

        let empty_layer_binding = tiles.empty_layer_binding(image.texel_type());
        let root_data = image.layer_stack().get_layer(image.root_id()).unwrap();
        let now = std::time::Instant::now();
        root_data.prepare_blend_cache(
            self,
            overriders,
            image,
            image.layer_stack().root_node(),
            tiles,
            &empty_layer_binding.texture,
            &empty_layer_binding.tile_info_buffer,
            &root_layer_binding.texture,
            &root_layer_binding.tile_info_buffer,
            device,
            queue,
        );
        log::debug!("Blend cache prepared in {:?}", now.elapsed());

        let now = std::time::Instant::now();
        root_data.dispatch_blend(
            self,
            &mut pass,
            image,
            image.layer_stack().root_node(),
            tiles,
        );
        log::debug!("Blend dispatched in {:?}", now.elapsed());

        drop(pass);

        queue.submit([ec.finish()]);
    }

    pub fn get_blend_cache<T: Send + Sync + 'static>(&self, layer_id: &LayerId) -> Option<&T> {
        self.cache.get(layer_id)?.downcast_ref::<T>()
    }

    pub fn get_blend_cache_mut<T: Send + Sync + 'static>(
        &mut self,
        layer_id: &LayerId,
    ) -> Option<&mut T> {
        self.cache.get_mut(layer_id)?.downcast_mut::<T>()
    }

    pub fn insert_blend_cache<T: Send + Sync + 'static>(&mut self, layer_id: LayerId, cache: T) {
        self.cache.insert(layer_id, Box::new(cache));
    }
}

#[derive(Default)]
pub struct LayerPreviewOverriders {
    overriders: HashMap<LayerId, Box<dyn Any + Send + Sync>>,
}

impl LayerPreviewOverriders {
    pub fn new() -> Self {
        Self {
            overriders: HashMap::new(),
        }
    }

    pub fn get_overrider<T: Send + Sync + 'static>(&self, layer_id: &LayerId) -> Option<&T> {
        let overrider = self.overriders.get(layer_id)?;
        overrider.downcast_ref::<T>()
    }

    pub fn insert_overrider<T: Send + Sync + 'static>(&mut self, layer_id: LayerId, overrider: T) {
        self.overriders.insert(layer_id, Box::new(overrider));
    }

    pub fn remove_overrider(&mut self, layer_id: &LayerId) {
        self.overriders.remove(layer_id);
    }
}

impl Global for LayerPreviewOverriders {}

pub struct PixelPreviewOverrider {
    pub texture: TextureView,
    pub tile_info_buffer: Buffer,
}
