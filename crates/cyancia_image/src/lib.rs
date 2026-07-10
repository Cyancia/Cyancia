wesl::wesl_pkg!(pub image);

use std::{
    fs::File,
    io::{BufRead, BufReader, Seek},
    path::Path,
    rc::Rc,
};

use anyhow::Result;
use bevy_math::IRect;
use glam::{IVec2, UVec2};
use gpui::App;
use imagers::{ImageDecoder, ImageReader};
use moxcms::ColorProfile;
// TODO move CImage to another place to avoid this.
extern crate image as imagers;

use crate::{
    blend_modes::BlendMode,
    composite::{
        BlendFunction, BlendFunctionAppExt, BlendFunctionRegistry, LayerPreviewOverriders,
    },
    layer::{LayerData, LayerId, LayerNameGenerator, LayerStack, SpecialLayers},
    texel::TexelType,
    tile::GpuTileStorage,
};

pub mod blend_modes;
pub mod composite;
pub mod convert;
pub mod dynamic_intermediate_buffer;
pub mod layer;
pub mod scan_pixels;
pub mod texel;
pub mod tile;

pub fn init(cx: &mut App) {
    cx.set_global(GpuTileStorage::from_app(cx));
    cx.set_global(LayerPreviewOverriders::default());
    cx.set_global(BlendFunctionRegistry::default());

    for blend_mode in BlendMode::ALL {
        cx.add_blend_function(Rc::new(blend_mode));
    }

    tile::init(cx);
}

#[derive(Debug)]
pub struct CImage {
    size: UVec2,
    profile: ColorProfile,
    texel_type: TexelType,
    layers: LayerStack,
    name_generator: LayerNameGenerator,
    special_layers: SpecialLayers,
}

impl CImage {
    pub fn new(size: UVec2, profile: ColorProfile) -> Self {
        let layers = LayerStack::new();

        Self {
            size,
            profile,
            texel_type: TexelType::RGBA8,
            layers,
            name_generator: LayerNameGenerator::default(),
            special_layers: SpecialLayers::new(),
        }
    }

    pub fn from_layer(size: UVec2, layer: LayerData, profile: ColorProfile) -> Self {
        let layers = LayerStack::with_background_layer(layer);

        Self {
            size,
            profile,
            texel_type: TexelType::RGBA8,
            layers,
            name_generator: Default::default(),
            special_layers: SpecialLayers::new(),
        }
    }

    pub fn from_file(path: impl AsRef<Path>, tiles: &GpuTileStorage) -> Result<Self> {
        let path = path.as_ref();
        let name = path
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        let (img, profile) = Self::load_image_with_profile(BufReader::new(File::open(path)?))?;
        Ok(Self::from_image(img, profile, name, tiles))
    }

    pub fn load_image_with_profile<R: BufRead + Seek>(
        r: R,
    ) -> Result<(imagers::DynamicImage, ColorProfile)> {
        let mut decoder = ImageReader::new(r).with_guessed_format()?.into_decoder()?;
        let profile = match decoder.icc_profile()? {
            Some(buf) => ColorProfile::new_from_slice(&buf)?,
            None => ColorProfile::new_srgb(),
        };
        let img = imagers::DynamicImage::from_decoder(decoder)?;
        Ok((img, profile))
    }

    pub fn from_image(
        img: imagers::DynamicImage,
        profile: ColorProfile,
        name: String,
        tiles: &GpuTileStorage,
    ) -> Self {
        let size = UVec2::new(img.width(), img.height());
        let layer = LayerData::from_image(name, img, tiles, BlendMode::Normal.id());
        Self::from_layer(size, layer, profile)
    }

    pub fn next_name_of_layer(&mut self, base: String) -> String {
        self.name_generator.next_of(base)
    }

    pub fn layer_stack(&self) -> &LayerStack {
        &self.layers
    }

    pub fn layer_stack_mut(&mut self) -> &mut LayerStack {
        &mut self.layers
    }

    pub fn profile(&self) -> &ColorProfile {
        &self.profile
    }

    pub fn size(&self) -> UVec2 {
        self.size
    }

    pub fn texel_type(&self) -> TexelType {
        self.texel_type
    }

    pub fn selection_layer(&self) -> LayerId {
        self.special_layers.selection_layer()
    }

    pub fn image_tile_rect(&self) -> IRect {
        GpuTileStorage::pixel_rect_to_tile(IRect {
            min: IVec2::ZERO,
            max: self.size.as_ivec2(),
        })
    }
}
