use std::{
    cell::OnceCell,
    collections::{HashMap, HashSet},
    ops::Deref,
    sync::Arc,
};

use bevy_math::IRect;
use cyancia_render::buffer::BufferVec;
use cyancia_runtime::{
    Services,
    service::{FromRuntime, RenderContext, Service},
};
use dashmap::{DashMap, DashSet, Entry};
use encase::ShaderType;
use glam::{IVec2, Mat3, UVec2};
use iced_core::Rectangle;
use image::{DynamicImage, GenericImageView, RgbaImage};
use indexmap::{IndexMap, IndexSet};
use palette::{LinSrgba, Srgb, Srgba};
use parking_lot::{Mutex, RwLock};
use rayon::iter::{
    IndexedParallelIterator, IntoParallelRefIterator, ParallelBridge, ParallelIterator,
};
use uuid::Uuid;
use wgpu::{
    BindingResource, BufferUsages, Device, Extent3d, Origin3d, Queue, TexelCopyBufferInfo,
    TexelCopyBufferLayout, TexelCopyTextureInfo, Texture, TextureAspect, TextureDescriptor,
    TextureDimension, TextureFormat, TextureUsages, TextureView, TextureViewDescriptor,
    TextureViewDimension,
    util::{BufferInitDescriptor, DeviceExt},
};

use crate::{
    layer::{Layer, LayerId},
    texel::{RGBA8_FORMAT, TexelDepth, TexelFormat, TexelType},
};

// TODO: We are having this wrapper because rendering iced primitives doesn't allow bring external context
//       with lifetime.
#[derive(Clone)]
pub struct GpuTileStorage {
    inner: Arc<GpuTileStorageInner>,
}

impl Service for GpuTileStorage {}

impl FromRuntime for GpuTileStorage {
    fn from_runtime(runtime: &Services) -> Self {
        Self {
            inner: Arc::new(GpuTileStorageInner::from_runtime(runtime)),
        }
    }
}

impl Deref for GpuTileStorage {
    type Target = GpuTileStorageInner;

    fn deref(&self) -> &Self::Target {
        self.inner.as_ref()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TileIndex {
    pub layer: LayerId,
    pub coord: IVec2,
}

#[derive(Clone)]
pub struct Tile {
    pub index: TileIndex,
    pub texture: Arc<TextureView>,
}

#[derive(ShaderType, Clone, Copy)]
pub struct GpuTileInfo {
    pub index: IVec2,
    pub origin: IVec2,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GpuLayerInfo {
    pub texel_type: TexelType,
}

pub struct LayerBindingData {
    pub texture: Arc<TextureView>,
    pub tile_info_buffer: BufferVec<GpuTileInfo>,
}

pub struct GpuTileStorageInner {
    device: Arc<Device>,
    queue: Arc<Queue>,

    dummy_layers: HashMap<TexelType, DynamicLayerStorage>,
    layers: DashMap<LayerId, DynamicLayerStorage>,
}

// impl Service for GpuTileStorageInner {}

impl FromRuntime for GpuTileStorageInner {
    fn from_runtime(runtime: &Services) -> Self {
        let render_context = runtime.service::<RenderContext>();
        Self::new(render_context.device.clone(), render_context.queue.clone())
    }
}

impl GpuTileStorageInner {
    pub const TILE_SIZE: u32 = 256;
    pub const EMPTY_TILE_COORD: IVec2 = IVec2::new(
        i32::MAX / Self::TILE_SIZE as i32,
        i32::MAX / Self::TILE_SIZE as i32,
    );

    pub fn new(device: Arc<Device>, queue: Arc<Queue>) -> Self {
        let mut dummy_layers = HashMap::new();
        for texel_type in TexelType::ALL_POSSIBLE_FORMATS {
            let mut st = DynamicLayerStorage::new(
                device.clone(),
                queue.clone(),
                GpuLayerInfo { texel_type },
            );
            st.get_tile_or_allocate(Self::EMPTY_TILE_COORD);
            dummy_layers.insert(texel_type, st);
        }

        Self {
            device,
            queue,
            dummy_layers,
            layers: DashMap::new(),
        }
    }

    pub fn clear_layer(&self, layer_id: LayerId) {
        if let Some(mut layer) = self.layers.get_mut(&layer_id) {
            layer.clear();
        }
    }

    pub fn declare_layer(&self, layer_id: LayerId, info: GpuLayerInfo) {
        match self.layers.entry(layer_id) {
            Entry::Occupied(e) => {
                assert_eq!(e.get().layer_info, info, "Declare layer info mismatch")
            }
            Entry::Vacant(e) => {
                e.insert(DynamicLayerStorage::new(
                    self.device.clone(),
                    self.queue.clone(),
                    info,
                ));
            }
        }
    }

    pub fn layer_info(&self, layer_id: LayerId) -> Option<GpuLayerInfo> {
        self.layers.get(&layer_id).map(|l| l.layer_info().clone())
    }

    pub fn try_allocate_tile(&self, tile_index: TileIndex) -> Option<Arc<TextureView>> {
        self.layers
            .get_mut(&tile_index.layer)
            .map(|mut layer| layer.get_tile_or_allocate(tile_index.coord))
    }

    pub fn get_layer_binding_or_empty(&self, layer_id: LayerId) -> Option<LayerBindingData> {
        let layer = self.layers.get(&layer_id)?;

        Some(if let Some(texture) = layer.texture() {
            LayerBindingData {
                texture,
                tile_info_buffer: layer.tile_info_buffer_binding(),
            }
        } else {
            let dummy = self.dummy_layers.get(&layer.layer_info.texel_type).unwrap();
            LayerBindingData {
                texture: dummy.texture().unwrap(),
                tile_info_buffer: dummy.tile_info_buffer_binding(),
            }
        })
    }

    pub fn upload_image(&self, layer_id: LayerId, img: DynamicImage) {
        let width = img.width();
        let height = img.height();

        let layer_info = GpuLayerInfo {
            texel_type: TexelType {
                format: TexelFormat::Rgba,
                depth: TexelDepth::Bit8,
            },
        };
        self.declare_layer(layer_id, layer_info);
        let mut layer = self.layers.get_mut(&layer_id).unwrap();

        let mut ec = self.device.create_command_encoder(&Default::default());
        let n_tiles = Self::calc_tile_count(UVec2::new(width, height));
        layer.reserve(n_tiles.element_product());

        for y in 0..n_tiles.y {
            for x in 0..n_tiles.x {
                let tile_index = TileIndex {
                    layer: layer_id,
                    coord: IVec2::new(x as i32, y as i32),
                };
                let tile = layer.get_tile_or_allocate(tile_index.coord);
                let tile_layer = layer.get_tile_layer(tile_index.coord).unwrap();
                log::info!("Uploading tile: {:?}", tile_index);
                let origin = UVec2::new(x, y) * Self::TILE_SIZE;

                let sub_img = img.view(
                    origin.x,
                    origin.y,
                    Self::TILE_SIZE.min(width - origin.x),
                    Self::TILE_SIZE.min(height - origin.y),
                );
                let data = layer_info
                    .texel_type
                    .convert_image_to_wgpu(DynamicImage::from(sub_img.to_image()));

                let texture = self.device.create_texture_with_data(
                    &self.queue,
                    &TextureDescriptor {
                        label: Some("temp tile texture"),
                        size: Extent3d {
                            width: sub_img.width(),
                            height: sub_img.height(),
                            depth_or_array_layers: 1,
                        },
                        mip_level_count: 1,
                        sample_count: 1,
                        dimension: TextureDimension::D2,
                        format: layer_info.texel_type.wgpu_format(),
                        usage: TextureUsages::COPY_SRC | TextureUsages::COPY_DST,
                        view_formats: &[],
                    },
                    Default::default(),
                    bytemuck::cast_slice(&data),
                );

                ec.copy_texture_to_texture(
                    texture.as_image_copy(),
                    TexelCopyTextureInfo {
                        texture: tile.texture(),
                        mip_level: 0,
                        origin: Origin3d {
                            x: 0,
                            y: 0,
                            z: tile_layer,
                        },
                        aspect: TextureAspect::All,
                    },
                    Extent3d {
                        width: sub_img.width(),
                        height: sub_img.height(),
                        depth_or_array_layers: 1,
                    },
                );
            }
        }

        self.queue.submit([ec.finish()]);
    }

    pub fn pixel_rect_to_tile(pixel_rect: IRect) -> IRect {
        IRect {
            min: pixel_rect.min / IVec2::splat(Self::TILE_SIZE as i32),
            max: (pixel_rect.max - 1) / IVec2::splat(Self::TILE_SIZE as i32) + 1,
        }
    }

    pub fn snap_to_tile_grid(pixel_rect: IRect) -> IRect {
        let tile_rect = Self::pixel_rect_to_tile(pixel_rect);
        IRect {
            min: tile_rect.min * IVec2::splat(Self::TILE_SIZE as i32),
            max: tile_rect.max * IVec2::splat(Self::TILE_SIZE as i32),
        }
    }

    pub fn calc_tile_count(image_size: UVec2) -> UVec2 {
        UVec2::new(
            image_size.x.div_ceil(Self::TILE_SIZE),
            image_size.y.div_ceil(Self::TILE_SIZE),
        )
    }
}

pub struct DynamicLayerStorage {
    device: Arc<Device>,
    queue: Arc<Queue>,
    texture: Option<Arc<TextureView>>,
    tiles: IndexMap<IVec2, Arc<TextureView>>,
    tile_info_buffer: BufferVec<GpuTileInfo>,
    layer_info: GpuLayerInfo,
}

impl DynamicLayerStorage {
    pub const GROWTH_RATE: f32 = 1.5;
    pub const TILE_SIZE: u32 = GpuTileStorageInner::TILE_SIZE;

    pub fn new(device: Arc<Device>, queue: Arc<Queue>, info: GpuLayerInfo) -> Self {
        Self {
            device,
            queue,
            texture: None,
            tiles: IndexMap::new(),
            tile_info_buffer: BufferVec::new(
                Some("tile info buffer".into()),
                BufferUsages::STORAGE,
            ),
            layer_info: info,
        }
    }

    pub fn layer_info(&self) -> &GpuLayerInfo {
        &self.layer_info
    }

    pub fn get_tile(&self, coord: IVec2) -> Option<Arc<TextureView>> {
        self.tiles.get(&coord).cloned()
    }

    pub fn get_tile_layer(&self, coord: IVec2) -> Option<u32> {
        self.tiles.get_index_of(&coord).map(|i| i as u32)
    }

    pub fn tile_info_buffer_binding(&self) -> BufferVec<GpuTileInfo> {
        self.tile_info_buffer.clone()
    }

    pub fn texture(&self) -> Option<Arc<TextureView>> {
        self.texture.clone()
    }

    pub fn get_tile_or_allocate(&mut self, coord: IVec2) -> Arc<TextureView> {
        if let Some(tile) = self.tiles.get(&coord) {
            return tile.clone();
        }

        let tile = if let Some(texture) = self.texture.as_deref()
            && self.tiles.len() < texture.texture().depth_or_array_layers() as usize
        {
            let tile = Arc::new(texture.texture().create_view(&TextureViewDescriptor {
                base_array_layer: self.tiles.len() as u32,
                array_layer_count: Some(1),
                ..Default::default()
            }));

            self.tiles.insert(coord, tile.clone());

            tile
        } else {
            let next_size = match self.texture.as_deref() {
                Some(t) => {
                    (t.texture().depth_or_array_layers() as f32 * Self::GROWTH_RATE).ceil() as u32
                }
                None => 1,
            };
            let new_texture = self.device.create_texture(&TextureDescriptor {
                label: Some("tile texture"),
                size: Extent3d {
                    width: Self::TILE_SIZE,
                    height: Self::TILE_SIZE,
                    depth_or_array_layers: next_size,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: TextureDimension::D2,
                format: self.layer_info.texel_type.wgpu_format(),
                usage: TextureUsages::TEXTURE_BINDING
                    | TextureUsages::COPY_SRC
                    | TextureUsages::COPY_DST
                    | TextureUsages::STORAGE_BINDING,
                view_formats: &[],
            });

            if let Some(old_texture) = self.texture.as_deref() {
                let mut ce = self.device.create_command_encoder(&Default::default());
                ce.copy_texture_to_texture(
                    old_texture.texture().as_image_copy(),
                    new_texture.as_image_copy(),
                    Extent3d {
                        width: Self::TILE_SIZE,
                        height: Self::TILE_SIZE,
                        depth_or_array_layers: old_texture.texture().depth_or_array_layers(),
                    },
                );
                self.queue.submit([ce.finish()]);
            }

            self.texture = Some(Arc::new(new_texture.create_view(&TextureViewDescriptor {
                dimension: Some(TextureViewDimension::D2Array),
                ..Default::default()
            })));
            for (i, tile) in self.tiles.values_mut().enumerate() {
                *tile = Arc::new(new_texture.create_view(&TextureViewDescriptor {
                    base_array_layer: i as u32,
                    array_layer_count: Some(1),
                    ..Default::default()
                }));
            }

            let tile = Arc::new(new_texture.create_view(&TextureViewDescriptor {
                base_array_layer: self.tiles.len() as u32,
                array_layer_count: Some(1),
                ..Default::default()
            }));
            self.tiles.insert(coord, tile.clone());

            tile
        };

        self.tile_info_buffer.push(&GpuTileInfo {
            index: coord,
            origin: coord * IVec2::splat(Self::TILE_SIZE as i32),
        });
        self.tile_info_buffer.write_buffer(&self.device);

        tile
    }

    pub fn reserve(&mut self, additional: u32) {
        if additional == 0 {
            return;
        }

        let new_capacity = self
            .texture
            .as_ref()
            .map(|t| t.texture().depth_or_array_layers())
            .unwrap_or_default()
            + additional;
        let new_texture = self.device.create_texture(&TextureDescriptor {
            label: Some("tile texture"),
            size: Extent3d {
                width: Self::TILE_SIZE,
                height: Self::TILE_SIZE,
                depth_or_array_layers: new_capacity,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: TextureDimension::D2,
            format: self.layer_info.texel_type.wgpu_format(),
            usage: TextureUsages::TEXTURE_BINDING
                | TextureUsages::COPY_SRC
                | TextureUsages::COPY_DST
                | TextureUsages::STORAGE_BINDING,
            view_formats: &[],
        });

        if let Some(old_texture) = self.texture.as_deref() {
            let mut ce = self.device.create_command_encoder(&Default::default());
            ce.copy_texture_to_texture(
                old_texture.texture().as_image_copy(),
                new_texture.as_image_copy(),
                Extent3d {
                    width: Self::TILE_SIZE,
                    height: Self::TILE_SIZE,
                    depth_or_array_layers: old_texture.texture().depth_or_array_layers(),
                },
            );
            self.queue.submit([ce.finish()]);
        }

        self.texture = Some(Arc::new(new_texture.create_view(&TextureViewDescriptor {
            dimension: Some(TextureViewDimension::D2Array),
            ..Default::default()
        })));

        for (i, tile) in self.tiles.values_mut().enumerate() {
            *tile = Arc::new(new_texture.create_view(&TextureViewDescriptor {
                base_array_layer: i as u32,
                array_layer_count: Some(1),
                ..Default::default()
            }));
        }
    }

    pub fn clear(&mut self) {
        self.tiles.clear();
        self.tile_info_buffer.clear();
        self.tile_info_buffer.write_buffer(&self.device);
    }
}
