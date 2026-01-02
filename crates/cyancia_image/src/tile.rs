use std::{cell::OnceCell, collections::HashMap, ops::Deref, sync::Arc};

use cyancia_id::Id;
use cyancia_utils::global_instance::GlobalInstance;
use dashmap::DashMap;
use glam::{Mat3, UVec2};
use iced_core::Rectangle;
use image::{DynamicImage, GenericImageView, RgbaImage};
use palette::{LinSrgba, Srgb, Srgba};
use parking_lot::RwLock;
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

use crate::layer::Layer;

#[derive(Debug)]
pub struct GpuTilePile {
    pub texture: Arc<Texture>,
    pub texture_view: Arc<TextureView>,
}

#[derive(Debug)]
pub struct GroupedTileViews {
    pub pile: Arc<TextureView>,
    pub tiles: Vec<TileId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TileId {
    pub image_layer: Id<Layer>,
    pub index: UVec2,
    pub pile_index: usize,
    pub pile_layer: u32,
}

#[derive(Debug, Clone)]
pub struct Tile {
    pub id: TileId,
    pub view: Arc<TextureView>,
}

pub static GPU_TILE_STORAGE: GlobalInstance<GpuTileStorage> = GlobalInstance::new();

#[derive(Debug)]
pub struct GpuTileStorage {
    device: Arc<Device>,
    queue: Arc<Queue>,
    piles: RwLock<Vec<GpuTilePile>>,
    tiles: DashMap<(Id<Layer>, UVec2), Tile>,
    available_slices: RwLock<Vec<(usize, usize)>>,
}

impl GpuTileStorage {
    pub const TILE_SIZE: u32 = 256;
    pub const TILES_PER_PILE: u32 = 256;
    pub const EMPTY_TILE_ID: TileId = TileId {
        image_layer: Id::from_uuid(Uuid::from_u128(0)),
        index: UVec2::ZERO,
        pile_layer: 0,
        pile_index: 0,
    };
    pub const TILE_FORMAT: TextureFormat = TextureFormat::Rgba16Float;

    pub fn calc_tile_count(image_size: UVec2) -> UVec2 {
        UVec2::new(
            image_size.x.div_ceil(Self::TILE_SIZE),
            image_size.y.div_ceil(Self::TILE_SIZE),
        )
    }

    pub fn new(device: Arc<Device>, queue: Arc<Queue>) -> Self {
        let empty_tile = device.create_texture(&TextureDescriptor {
            label: Some("empty tile"),
            size: Extent3d {
                width: Self::TILE_SIZE,
                height: Self::TILE_SIZE,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: TextureDimension::D2,
            format: Self::TILE_FORMAT,
            usage: TextureUsages::TEXTURE_BINDING
                | TextureUsages::COPY_DST
                | TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });

        let empty_tile_view = empty_tile.create_view(&TextureViewDescriptor {
            label: Some("empty tile view"),
            format: None,
            dimension: None,
            aspect: wgpu::TextureAspect::All,
            base_mip_level: 0,
            mip_level_count: None,
            base_array_layer: 0,
            array_layer_count: None,
            usage: None,
        });

        let views = DashMap::from_iter([(
            (Self::EMPTY_TILE_ID.image_layer, Self::EMPTY_TILE_ID.index),
            Tile {
                id: Self::EMPTY_TILE_ID,
                view: empty_tile_view.into(),
            },
        )]);

        let piles = vec![GpuTilePile {
            texture_view: empty_tile
                .create_view(&TextureViewDescriptor {
                    label: Some("empty pile view"),
                    format: None,
                    dimension: Some(wgpu::TextureViewDimension::D2Array),
                    usage: None,
                    aspect: wgpu::TextureAspect::All,
                    base_mip_level: 0,
                    mip_level_count: None,
                    base_array_layer: 0,
                    array_layer_count: None,
                })
                .into(),
            texture: empty_tile.into(),
        }];

        Self {
            device,
            queue,
            piles: piles.into(),
            tiles: views,
            available_slices: Default::default(),
        }
    }

    pub fn get_tile(&self, image_layer: Id<Layer>, index: UVec2) -> Tile {
        self.tiles
            .get(&(image_layer, index))
            .map(|r| r.value().clone())
            .unwrap_or_else(|| {
                let mut empty = self
                    .tiles
                    .get(&(Self::EMPTY_TILE_ID.image_layer, Self::EMPTY_TILE_ID.index))
                    .unwrap()
                    .value()
                    .clone();
                empty.id.index = index;
                empty
            })
    }

    pub fn get_tile_mut(&self, image_layer: Id<Layer>, index: UVec2) -> Tile {
        dbg!(self.tiles.len(), self.available_slices.read().len());
        match self.tiles.entry((image_layer, index)) {
            dashmap::Entry::Occupied(e) => e.get().clone(),
            dashmap::Entry::Vacant(e) => {
                self.try_allocate_new_tile_pile();
                let (pile_index, slice_index) = self.available_slices.write().pop().unwrap();
                let pile = &self.piles.read()[pile_index];
                let view = pile.texture.create_view(&TextureViewDescriptor {
                    label: Some("tile view"),
                    format: None,
                    dimension: Some(wgpu::TextureViewDimension::D2),
                    aspect: wgpu::TextureAspect::All,
                    base_mip_level: 0,
                    mip_level_count: None,
                    base_array_layer: slice_index as u32,
                    array_layer_count: Some(1),
                    usage: None,
                });
                dbg!(slice_index);

                let tile = Tile {
                    id: TileId {
                        image_layer,
                        index,
                        pile_index,
                        pile_layer: slice_index as u32,
                    },
                    view: view.clone().into(),
                };
                e.insert(tile.clone());
                tile
            }
        }
    }

    fn try_allocate_new_tile_pile(&self) {
        if !self.available_slices.read().is_empty() {
            return;
        }

        let texture = self.device.create_texture(&TextureDescriptor {
            label: Some("pile"),
            size: Extent3d {
                width: Self::TILE_SIZE,
                height: Self::TILE_SIZE,
                depth_or_array_layers: Self::TILES_PER_PILE,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: TextureDimension::D2,
            format: Self::TILE_FORMAT,
            usage: TextureUsages::STORAGE_BINDING
                | TextureUsages::COPY_DST
                | TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let texture_view = texture.create_view(&TextureViewDescriptor {
            label: Some("pile view"),
            format: None,
            dimension: None,
            usage: None,
            aspect: TextureAspect::All,
            base_mip_level: 0,
            mip_level_count: None,
            base_array_layer: 0,
            array_layer_count: None,
        });

        let mut piles = self.piles.write();
        piles.push(GpuTilePile {
            texture: Arc::new(texture),
            texture_view: Arc::new(texture_view),
        });
        log::info!(
            "Allocated new tile pile. Current pile count: {}",
            piles.len()
        );
        let pile_index = piles.len() - 1;
        self.available_slices
            .write()
            .extend((0..Self::TILES_PER_PILE as usize).map(|x| (pile_index, x)));
    }

    pub fn upload_image(&self, layer_id: Id<Layer>, img: DynamicImage) {
        let width = img.width();
        let height = img.height();

        let img = img.into_rgba32f();

        let required_tile_count = Self::calc_tile_count(UVec2::new(width, height));
        let target_tiles = (0..required_tile_count.x)
            .flat_map(|x| {
                (0..required_tile_count.y)
                    .map(move |y| self.get_tile_mut(layer_id, UVec2::new(x, y)))
            })
            .collect::<Vec<_>>();

        let mut ec = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("upload tile encoder"),
            });

        for tile in target_tiles {
            log::info!("Uploading tile: {:?}", tile.id.index);
            let origin = tile.id.index * Self::TILE_SIZE;

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
                TexelCopyTextureInfo {
                    texture: tile.view.texture(),
                    mip_level: 0,
                    origin: Origin3d {
                        x: 0,
                        y: 0,
                        z: tile.id.pile_layer,
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

    pub fn get_tile_views(
        &self,
        pixel_rect: Rectangle<u32>,
        total_tile_count: UVec2,
        image_layer: Id<Layer>,
    ) -> Vec<GroupedTileViews> {
        let pixel_min = UVec2::new(pixel_rect.x, pixel_rect.y);
        let pixel_max = UVec2::new(
            pixel_rect.x + pixel_rect.width,
            pixel_rect.y + pixel_rect.height,
        );
        let min = pixel_min / Self::TILE_SIZE;
        let max = UVec2::new(
            pixel_max.x.div_ceil(Self::TILE_SIZE),
            pixel_max.y.div_ceil(Self::TILE_SIZE),
        )
        .min(total_tile_count - 1);

        let groups = (min.x..=max.x)
            .flat_map(move |x| {
                (min.y..=max.y).map(move |y| self.get_tile(image_layer, UVec2::new(x, y)))
            })
            .fold(HashMap::new(), |mut acc, tile| {
                acc.entry(tile.id.pile_index)
                    .or_insert_with(Vec::new)
                    .push(tile.id);
                acc
            });

        let piles = self.piles.read();
        groups
            .into_iter()
            .map(|(pile_index, tiles)| GroupedTileViews {
                pile: piles[pile_index].texture_view.clone(),
                tiles,
            })
            .collect()
    }
}
