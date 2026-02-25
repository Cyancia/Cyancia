use cyancia_utils::wrapper;
use glam::UVec2;
use image::DynamicImage;
use uuid::Uuid;
use wgpu::TextureFormat;

use crate::tile::GpuTileStorage;

wrapper! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub LayerId : Uuid
}

#[derive(Debug)]
pub struct Layer {
    pub id: LayerId,
    pub size: UVec2,
}

impl Layer {
    pub fn new() -> Self {
        Self {
            id: LayerId::new(Uuid::new_v4()),
            size: UVec2::ZERO,
        }
    }

    pub fn id(&self) -> LayerId {
        self.id
    }

    pub fn from_image(img: DynamicImage, tiles: &GpuTileStorage) -> Self {
        let id = LayerId::new(Uuid::new_v4());
        let size = UVec2::new(img.width(), img.height());
        tiles.upload_image(id, img);

        Self { id, size }
    }
}
