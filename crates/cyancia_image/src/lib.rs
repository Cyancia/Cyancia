wesl::wesl_pkg!(pub image);

use std::path::Path;

use glam::UVec2;
use gpui::App;
// TODO move CImage to another place to avoid this.
extern crate image as imagers;

use crate::{
    blend_modes::BlendMode,
    composite::LayerPreviewOverriders,
    layer::{LayerData, LayerId, LayerNameGenerator, LayerStack, LayerStackNode},
    texel::TexelType,
    tile::GpuTileStorage,
};

pub mod blend_modes;
pub mod composite;
pub mod dynamic_intermediate_buffer;
pub mod layer;
pub mod texel;
pub mod tile;

pub fn init(cx: &mut App) {
    cx.set_global(GpuTileStorage::from_app(cx));
    cx.set_global(LayerPreviewOverriders::default());
}

#[derive(Debug)]
pub struct CImage {
    size: UVec2,
    pub active_layer: LayerId,
    texel_type: TexelType,
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
            texel_type: TexelType::RGBA8,
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
            texel_type: TexelType::RGBA8,
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

    pub fn next_name_of_layer(&mut self, base: String) -> String {
        self.name_generator.next_of(base)
    }

    pub fn parent_of_active_layer(&self) -> LayerId {
        let l = self
            .layers
            .find_node(self.active_layer)
            .expect("Active layer should always exist");
        l.parent()
            .expect("Active layer should always have a parent")
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

    pub fn texel_type(&self) -> TexelType {
        self.texel_type
    }

    pub fn active_layer_node(&self) -> &LayerStackNode {
        self.layers
            .find_node(self.active_layer)
            .expect("Active layer should always exist")
    }

    pub fn active_layer_node_mut(&mut self) -> &mut LayerStackNode {
        self.layers
            .find_node_mut(self.active_layer)
            .expect("Active layer should always exist")
    }

    pub fn active_layer_data(&self) -> &LayerData {
        self.layers
            .get_layer(self.active_layer)
            .expect("Active layer should always exist")
    }

    pub fn active_layer_data_mut(&mut self) -> &mut LayerData {
        self.layers
            .get_layer_mut(self.active_layer)
            .expect("Active layer should always exist")
    }
}
