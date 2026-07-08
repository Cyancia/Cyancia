wesl::wesl_pkg!(pub image);

use std::{path::Path, rc::Rc};

use bevy_math::IRect;
use glam::{IVec2, UVec2};
use gpui::App;
use moxcms::{CicpProfile, ColorProfile};
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

    pub fn from_file(path: impl AsRef<Path>, tiles: &GpuTileStorage) -> imagers::ImageResult<Self> {
        let path = path.as_ref();
        let name = path
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        Ok(Self::from_image(imagers::open(path)?, name, tiles))
    }

    pub fn from_image(img: imagers::DynamicImage, name: String, tiles: &GpuTileStorage) -> Self {
        let cicp = img.color_space();
        let mut profile = ColorProfile::new_srgb();
        // TODO Really confused why image is not making the to_moxcms_compute_profile public
        profile.update_rgb_colorimetry_from_cicp(CicpProfile {
            color_primaries: {
                use imagers::metadata::CicpColorPrimaries as I;
                use moxcms::CicpColorPrimaries as M;

                match cicp.primaries {
                    I::SRgb => M::Bt709,
                    I::Unspecified => M::Unspecified,
                    I::RgbM => M::Bt470M,
                    I::RgbB => M::Bt470Bg,
                    I::Bt601 => M::Bt601,
                    I::Rgb240m => M::Smpte240,
                    I::GenericFilm => M::GenericFilm,
                    I::Rgb2020 => M::Bt2020,
                    I::Xyz => M::Xyz,
                    I::SmpteRp431 => M::Smpte431,
                    I::SmpteRp432 => M::Smpte432,
                    I::Industry22 => M::Ebu3213,
                    _ => unimplemented!(),
                }
            },
            transfer_characteristics: {
                use imagers::metadata::CicpTransferCharacteristics as I;
                use moxcms::TransferCharacteristics as T;

                match cicp.transfer {
                    I::Bt709 => T::Bt709,
                    I::Unspecified => T::Unspecified,
                    I::Bt470M => T::Bt470M,
                    I::Bt470BG => T::Bt470Bg,
                    I::Bt601 => T::Bt601,
                    I::Smpte240m => T::Smpte240,
                    I::Linear => T::Linear,
                    I::Log100 => T::Log100,
                    I::LogSqrt => T::Log100sqrt10,
                    I::Iec61966_2_4 => T::Iec61966,
                    I::Bt1361 => T::Bt1361,
                    I::SRgb => T::Srgb,
                    I::Bt2020_10bit => T::Bt202010bit,
                    I::Bt2020_12bit => T::Bt202012bit,
                    I::Smpte2084 => T::Smpte2084,
                    I::Smpte428 => T::Smpte428,
                    I::Bt2100Hlg => T::Hlg,
                    _ => unimplemented!(),
                }
            },
            matrix_coefficients: {
                use imagers::metadata::CicpMatrixCoefficients as I;
                use moxcms::MatrixCoefficients as M;

                match cicp.matrix {
                    I::Identity => M::Identity,
                    I::Unspecified => M::Unspecified,
                    I::Bt709 => M::Bt709,
                    I::UsFCC => M::Fcc,
                    I::Bt470BG => M::Bt470Bg,
                    I::Smpte170m => M::Smpte170m,
                    I::Smpte240m => M::Smpte240m,
                    I::YCgCo => M::YCgCo,
                    I::Bt2020NonConstant => M::Bt2020Ncl,
                    I::Bt2020Constant => M::Bt2020Cl,
                    I::Smpte2085 => M::Smpte2085,
                    I::ChromaticityDerivedNonConstant => M::ChromaticityDerivedNCL,
                    I::ChromaticityDerivedConstant => M::ChromaticityDerivedCL,
                    I::Bt2100 => M::ICtCp,
                    I::IptPqC2 | I::YCgCoRe | I::YCgCoRo => {
                        unimplemented!("Unsupported")
                    }
                    _ => unimplemented!(),
                }
            },
            full_range: match cicp.full_range {
                imagers::metadata::CicpVideoFullRangeFlag::NarrowRange => false,
                imagers::metadata::CicpVideoFullRangeFlag::FullRange => true,
                _ => unimplemented!(),
            },
        });

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
