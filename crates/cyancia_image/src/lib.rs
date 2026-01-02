use std::path::Path;

use glam::UVec2;
use image::DynamicImage;

use crate::layer::Layer;

pub mod layer;
pub mod tile;

#[derive(Debug)]
pub struct CImage {
    size: UVec2,
    root: Layer,
}

impl CImage {
    pub fn new(size: UVec2) -> Self {
        Self {
            size,
            root: Layer::new(),
        }
    }

    pub fn from_layer(size: UVec2, root: Layer) -> Self {
        Self { size, root }
    }

    pub fn from_file(path: impl AsRef<Path>) -> image::ImageResult<Self> {
        Ok(Self::from_dynamic(image::open(path)?))
    }

    pub fn from_dynamic(img: DynamicImage) -> Self {
        let size = UVec2::new(img.width(), img.height());
        Self {
            size,
            root: Layer::new(),
        }
    }

    pub fn size(&self) -> UVec2 {
        self.size
    }

    pub fn root(&self) -> &Layer {
        &self.root
    }
}
