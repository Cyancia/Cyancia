wesl::wesl_pkg!(pub image);

use std::path::Path;

use cyancia_runtime::{Application, Runtime, plugin::Plugin};
use glam::UVec2;
// TODO move CImage to another place to avoid this.
extern crate image as imagers;

use crate::{layer::Layer, tile::GpuTileStorage};

pub mod blend_modes;
pub mod layer;
pub mod texel;
pub mod tile;

pub struct ImagePlugin;

impl Plugin for ImagePlugin {
    fn build(&self, app: &mut Application) {
        app.add_service::<GpuTileStorage>();
    }
}

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

    pub fn from_file(path: impl AsRef<Path>) -> imagers::ImageResult<Self> {
        Ok(Self::from_dynamic(imagers::open(path)?))
    }

    pub fn from_dynamic(img: imagers::DynamicImage) -> Self {
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
