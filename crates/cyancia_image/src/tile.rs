use std::{cell::OnceCell, collections::HashMap, ops::Deref, sync::Arc};

use bevy_math::IRect;
use cyancia_runtime::{
    Services,
    service::{FromRuntime, RenderContext, Service},
};
use dashmap::DashMap;
use glam::{IVec2, Mat3, UVec2};
use iced_core::Rectangle;
use image::{DynamicImage, GenericImageView, RgbaImage};
use palette::{LinSrgba, Srgb, Srgba};
use parking_lot::{Mutex, RwLock};
use rayon::iter::{
    IndexedParallelIterator, IntoParallelRefIterator, ParallelBridge, ParallelIterator,
};
use uuid::Uuid;
use wgpu::{
    BufferUsages, Device, Extent3d, Origin3d, Queue, TexelCopyBufferInfo, TexelCopyBufferLayout,
    TexelCopyTextureInfo, Texture, TextureAspect, TextureDescriptor, TextureDimension,
    TextureFormat, TextureUsages, TextureView, TextureViewDescriptor,
    util::{BufferInitDescriptor, DeviceExt},
    wgt::TextureDataOrder,
};

use crate::layer::{Layer, LayerId};

#[derive(Debug, Clone)]
pub struct Tile {
    pub texture: Arc<Texture>,
    pub view: Arc<TextureView>,
}

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

#[derive(Debug)]
pub struct GpuTileStorageInner {
    device: Arc<Device>,
    queue: Arc<Queue>,

    empty_tile: Tile,
    tiles: DashMap<TileIndex, Tile>,
}

// impl Service for GpuTileStorageInner {}

impl FromRuntime for GpuTileStorageInner {
    fn from_runtime(runtime: &Services) -> Self {
        let render_context = runtime.service::<RenderContext>();
        Self::new(render_context.device.clone(), render_context.queue.clone())
    }
}

fn create_tile(device: &Device) -> Tile {
    let t = device.create_texture(&GpuTileStorageInner::TILE_DESC);
    let v = t.create_view(&Default::default());

    Tile {
        texture: Arc::new(t),
        view: Arc::new(v),
    }
}

impl GpuTileStorageInner {
    pub const TILE_SIZE: u32 = 256;
    pub const TILES_PER_PILE: u32 = 256;
    pub const EMPTY_TILE_ID: TileIndex = TileIndex {
        layer: LayerId::new(Uuid::nil()),
        coord: IVec2::new(u32::MAX as i32, u32::MAX as i32),
    };
    pub const TILE_FORMAT: TextureFormat = TextureFormat::Rgba16Float;
    pub const TILE_DESC: TextureDescriptor<'static> = TextureDescriptor {
        label: Some("tile texture"),
        size: Extent3d {
            width: Self::TILE_SIZE,
            height: Self::TILE_SIZE,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: TextureDimension::D2,
        format: Self::TILE_FORMAT,
        usage: TextureUsages::from_bits_truncate(
            TextureUsages::TEXTURE_BINDING.bits()
                | TextureUsages::COPY_DST.bits()
                | TextureUsages::STORAGE_BINDING.bits(),
        ),
        view_formats: &[],
    };

    pub fn calc_tile_count(image_size: UVec2) -> UVec2 {
        UVec2::new(
            image_size.x.div_ceil(Self::TILE_SIZE),
            image_size.y.div_ceil(Self::TILE_SIZE),
        )
    }

    pub fn new(device: Arc<Device>, queue: Arc<Queue>) -> Self {
        let empty = create_tile(&device);
        let tiles = DashMap::from_iter([(Self::EMPTY_TILE_ID, empty.clone())]);

        Self {
            device,
            queue,
            empty_tile: empty,
            tiles,
        }
    }

    pub fn get_tile(&self, index: TileIndex) -> Tile {
        self.tiles
            .get(&index)
            .as_deref()
            .cloned()
            .unwrap_or_else(|| self.empty_tile.clone())
    }

    pub fn get_tile_mut(&self, index: TileIndex) -> Tile {
        self.tiles
            .entry(index)
            .or_insert_with(|| create_tile(&self.device))
            .clone()
    }

    pub fn upload_image(&self, layer_id: LayerId, img: DynamicImage) {
        let width = img.width();
        let height = img.height();

        let img = img.into_rgba32f();

        let mut ec = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("upload tile encoder"),
            });

        for y in 0..width.div_ceil(Self::TILE_SIZE) {
            for x in 0..height.div_ceil(Self::TILE_SIZE) {
                let tile_index = TileIndex {
                    layer: layer_id,
                    coord: IVec2::new(x as i32, y as i32),
                };
                let tile = self.get_tile_mut(tile_index);
                log::info!("Uploading tile: {:?}", tile_index);
                let origin = UVec2::new(x, y) * Self::TILE_SIZE;

                let sub_img = img.view(
                    origin.x,
                    origin.y,
                    Self::TILE_SIZE.min(width - origin.x),
                    Self::TILE_SIZE.min(height - origin.y),
                );
                let data = sub_img
                    .pixels()
                    .flat_map(|(_, _, px)| px.0.map(|x| half::f16::from_f32(x).to_bits()))
                    .collect::<Vec<_>>();

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
                        format: TextureFormat::Rgba16Float,
                        usage: TextureUsages::COPY_SRC | TextureUsages::COPY_DST,
                        view_formats: &[],
                    },
                    Default::default(),
                    bytemuck::cast_slice(&data),
                );

                ec.copy_texture_to_texture(
                    texture.as_image_copy(),
                    tile.texture.as_image_copy(),
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

    // pub fn offload_tile(&self, tile_id: TileId, callback: impl FnOnce(Vec<u8>) + Send + 'static) {
    //     let Some((id, tile_view)) = self.views.remove(&tile_id) else {
    //         return;
    //     };
    //     let texture = tile_view.texture_view.texture();
    //     let pixel_size = texture.format().block_copy_size(None).unwrap();
    //     let buffer = self.device.create_buffer(BufferDescriptor {
    //         label: Some("temp buffer"),
    //         size: (texture.width() * texture.height() * pixel_size) as wgpu::BufferAddress,
    //         usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
    //         mapped_at_creation: false,
    //     });
    //     let mut ce = self
    //         .device
    //         .create_command_encoder(CommandEncoderDescriptor { label: None });
    //     ce.copy_texture_to_buffer(
    //         wgpu::TexelCopyTextureInfo {
    //             texture,
    //             mip_level: 1,
    //             aspect: wgpu::TextureAspect::All,
    //             origin: wgpu::Origin3d {
    //                 x: 0,
    //                 y: 0,
    //                 z: tile_view.texture_layer,
    //             },
    //         },
    //         wgpu::TexelCopyBufferInfo {
    //             buffer: &buffer,
    //             layout: wgpu::TexelCopyBufferLayout {
    //                 offset: 0,
    //                 bytes_per_row: Some(texture.width() * pixel_size),
    //                 rows_per_image: None,
    //             },
    //         },
    //         wgpu::Extent3d {
    //             width: texture.width(),
    //             height: texture.height(),
    //             depth_or_array_layers: 1,
    //         },
    //     );
    //     self.queue.submit([ce.finish()]);
    //     buffer
    //         .clone()
    //         .map_async(wgpu::MapMode::Read, .., move |result| {
    //             if let Err(e) = result {
    //                 return;
    //             }

    //             let data = buffer.slice(..).get_mapped_range().to_vec();
    //             buffer.unmap();
    //             callback(data);
    //         });
    // }

    // pub fn get_tile_views(
    //     &self,
    //     pixel_rect: Rectangle<u32>,
    //     total_tile_count: UVec2,
    //     image_layer: LayerId,
    // ) -> Vec<GroupedTileViews> {
    //     let pixel_min = UVec2::new(pixel_rect.x, pixel_rect.y);
    //     let pixel_max = UVec2::new(
    //         pixel_rect.x + pixel_rect.width,
    //         pixel_rect.y + pixel_rect.height,
    //     );
    //     let min = pixel_min / Self::TILE_SIZE;
    //     let max = UVec2::new(
    //         pixel_max.x.div_ceil(Self::TILE_SIZE),
    //         pixel_max.y.div_ceil(Self::TILE_SIZE),
    //     )
    //     .min(total_tile_count - 1);

    //     let groups = (min.x..=max.x)
    //         .flat_map(move |x| {
    //             (min.y..=max.y).map(move |y| self.get_tile(image_layer, UVec2::new(x, y)))
    //         })
    //         .fold(HashMap::new(), |mut acc, tile| {
    //             acc.entry(tile.id.pile_index)
    //                 .or_insert_with(Vec::new)
    //                 .push(tile.id);
    //             acc
    //         });

    //     let piles = self.piles.read();
    //     groups
    //         .into_iter()
    //         .map(|(pile_index, tiles)| GroupedTileViews {
    //             pile: piles[pile_index].texture_view.clone(),
    //             tiles,
    //         })
    //         .collect()
    // }

    pub fn get_tiles_ordered_by_tile_rect(&self, layer_id: LayerId, tile_rect: IRect) -> Vec<Tile> {
        (tile_rect.min.y..tile_rect.max.y)
            .flat_map(|y| {
                (tile_rect.min.x..tile_rect.max.x).map(move |x| {
                    self.get_tile(TileIndex {
                        layer: layer_id,
                        coord: IVec2::new(x, y),
                    })
                })
            })
            .collect()
    }

    pub fn get_tiles_ordered(&self, layer_id: LayerId, pixel_rect: IRect) -> Vec<Tile> {
        let tile_min = pixel_rect.min / IVec2::splat(Self::TILE_SIZE as i32);
        let tile_max = pixel_rect.max / IVec2::splat(Self::TILE_SIZE as i32);

        (tile_min.y..tile_max.y)
            .flat_map(|y| {
                (tile_min.x..tile_max.x).map(move |x| {
                    self.get_tile(TileIndex {
                        layer: layer_id,
                        coord: IVec2::new(x, y),
                    })
                })
            })
            .collect()
    }
}
