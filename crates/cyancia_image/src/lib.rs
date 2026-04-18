wesl::wesl_pkg!(pub image);

use std::path::Path;

use cyancia_runtime::{Application, Runtime, plugin::Plugin};
use glam::UVec2;
// TODO move CImage to another place to avoid this.
extern crate image as imagers;

use crate::{
    blend_modes::BlendMode,
    layer::{LayerData, LayerId, LayerNameGenerator, LayerStack},
    tile::GpuTileStorage,
};

pub mod blend_modes;
pub mod composite;
pub mod dynamic_intermediate_buffer;
pub mod layer;
pub mod texel;
pub mod tile;
pub mod widget;

pub struct ImagePlugin;

impl Plugin for ImagePlugin {
    fn build(&self, app: &mut Application) {
        app.add_service::<GpuTileStorage>();
    }
}

#[derive(Debug)]
pub struct CImage {
    size: UVec2,
    pub active_layer: LayerId,
    layers: LayerStack,
    name_generator: LayerNameGenerator,
}

impl CImage {
    pub fn new(size: UVec2) -> Self {
        let layers = LayerStack::new();
        let active_layer = layers
            .root_node()
            .children()
            .first()
            .expect("Background layer should be created by default")
            .id();

        Self {
            size,
            active_layer,
            layers,
            name_generator: LayerNameGenerator::default(),
        }
    }

    pub fn from_layer(size: UVec2, layer: LayerData) -> Self {
        let layers = LayerStack::with_background_layer(layer);
        let active_layer = layers
            .root_node()
            .children()
            .first()
            .expect("Background layer should be created by default")
            .id();

        Self {
            size,
            active_layer,
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
        let layer = LayerData::from_image(name, img, tiles, Box::new(BlendMode::Normal));
        Self::from_layer(size, layer)
    }

    pub fn insert_new_layer(&mut self, parent: LayerId, layer: LayerData) {
        self.layers.add_layer(parent, layer);
    }

    pub fn next_name_of_layer(&mut self, base: String) -> String {
        self.name_generator.next_of(base)
    }

    pub fn parent_of_active_layer(&self) -> Option<LayerId> {
        let l = self.layers.find_node(self.active_layer)?;
        l.parent()
    }

    pub fn layer_stack(&self) -> &LayerStack {
        &self.layers
    }

    pub fn layer_stack_mut(&mut self) -> &mut LayerStack {
        &mut self.layers
    }

    pub fn size(&self) -> UVec2 {
        self.size
    }

    pub fn root_id(&self) -> LayerId {
        self.layers.root_id()
    }
}
