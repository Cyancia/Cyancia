use cyancia_id::Id;
use glam::UVec2;
use image::DynamicImage;
use wgpu::TextureFormat;

use crate::tile::GpuTileStorage;

#[derive(Debug)]
pub struct Layer {
    pub id: Id<Layer>,
    pub size: UVec2,
}

impl Layer {
    pub fn new() -> Self {
        Self {
            id: Id::random(),
            size: UVec2::ZERO,
        }
    }

    pub fn id(&self) -> Id<Layer> {
        self.id
    }

    pub fn from_image(img: DynamicImage, tiles: &GpuTileStorage) -> Self {
        let id = Id::random();
        let size = UVec2::new(img.width(), img.height());
        tiles.upload_image(id, img);

        Self { id, size }
    }
}
