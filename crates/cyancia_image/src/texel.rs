use imagers::DynamicImage;
use wgpu::{TextureFormat, TextureSampleType};

pub const A8_FORMAT: TextureFormat = TextureFormat::R8Unorm;
pub const RGBA8_FORMAT: TextureFormat = TextureFormat::R32Uint;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TexelFormat {
    Alpha,
    Rgba,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TexelDepth {
    Bit8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TexelType {
    pub format: TexelFormat,
    pub depth: TexelDepth,
}

impl TexelType {
    pub const RGBA8: Self = Self {
        format: TexelFormat::Rgba,
        depth: TexelDepth::Bit8,
    };

    pub const A8: Self = Self {
        format: TexelFormat::Alpha,
        depth: TexelDepth::Bit8,
    };

    pub const ALL_POSSIBLE_FORMATS: [Self; 2] = [Self::RGBA8, Self::A8];

    pub fn wgpu_format(&self) -> TextureFormat {
        match (self.format, self.depth) {
            (TexelFormat::Rgba, TexelDepth::Bit8) => RGBA8_FORMAT,
            (TexelFormat::Alpha, TexelDepth::Bit8) => A8_FORMAT,
        }
    }

    pub fn shader_def(&self) -> &'static str {
        match (self.format, self.depth) {
            (TexelFormat::Rgba, TexelDepth::Bit8) => "RGBA8",
            (TexelFormat::Alpha, TexelDepth::Bit8) => "A8",
        }
    }

    pub fn sample_type(&self) -> TextureSampleType {
        match (self.format, self.depth) {
            (TexelFormat::Rgba, TexelDepth::Bit8) => TextureSampleType::Uint,
            (TexelFormat::Alpha, TexelDepth::Bit8) => TextureSampleType::Float { filterable: true },
        }
    }

    pub fn alpha_channel_index(&self) -> u32 {
        match self.format {
            TexelFormat::Rgba => 3,
            TexelFormat::Alpha => 0,
        }
    }

    pub fn convert_image_to_wgpu(&self, img: DynamicImage) -> Vec<u8> {
        match (self.format, self.depth) {
            (TexelFormat::Rgba, TexelDepth::Bit8) => {
                let rgba8 = img.to_rgba8();
                rgba8.into_raw()
            }
            (TexelFormat::Alpha, TexelDepth::Bit8) => {
                let alpha8 = img.to_luma8();
                alpha8.into_raw()
            }
        }
    }
}
