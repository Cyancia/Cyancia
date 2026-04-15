wesl::wesl_pkg!(pub image);

use std::path::Path;

use cyancia_runtime::{Application, Runtime, plugin::Plugin};
use glam::UVec2;
// TODO move CImage to another place to avoid this.
extern crate image as imagers;

use crate::{
    layer::{Layer, LayerId, LayerNameGenerator, LayerStack},
    tile::GpuTileStorage,
};

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
    layers: LayerStack,
    name_generator: LayerNameGenerator,
}

impl CImage {
    pub fn new(size: UVec2) -> Self {
        Self {
            size,
            layers: LayerStack::new(),
            name_generator: LayerNameGenerator::default(),
        }
    }

    pub fn from_layer(size: UVec2, layer: Layer) -> Self {
        let mut layers = LayerStack::new();
        layers.add_layer(layers.root(), layer);
        Self {
            size,
            layers,
            name_generator: Default::default(),
        }
    }

    pub fn from_file(path: impl AsRef<Path>, tiles: &GpuTileStorage) -> imagers::ImageResult<Self> {
        let path = path.as_ref();
        let name = path
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        Ok(Self::from_image(imagers::open(&path)?, name, tiles))
    }

    pub fn from_image(img: imagers::DynamicImage, name: String, tiles: &GpuTileStorage) -> Self {
        let size = UVec2::new(img.width(), img.height());
        let layer = Layer::from_image(name, img, tiles);
        Self::from_layer(size, layer)
    }

    pub fn new_layer(&mut self, name_prefix: String, parent: LayerId) -> LayerId {
        let name = self.name_generator.next_of(name_prefix);
        let layer = Layer::new(name);
        let id = layer.id();
        self.layers.add_layer(parent, layer);
        id
    }

    pub fn size(&self) -> UVec2 {
        self.size
    }

    pub fn root(&self) -> LayerId {
        self.layers.root()
    }
}
