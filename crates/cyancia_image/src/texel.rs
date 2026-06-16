use imagers::DynamicImage;
use wgpu::{TextureFormat, TextureSampleType};

// TODO: Add rgba16 and rgba32, gray8, gray16, gray32 in the future. Notice that, current render architecture assumes that
//       all texture buffers are rgba8 to make things simpler. Textures imported externally, such as brush textures, will
//       be converted into rgba8. See GpuTileStorage.
//       So this is a rgba8 program so far. ( . ‸ .)

pub const RGBA8_FORMAT: TextureFormat = TextureFormat::R32Uint;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TexelFormat {
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

    pub const ALL_POSSIBLE_FORMATS: [Self; 1] = [Self::RGBA8];

    pub fn wgpu_format(&self) -> TextureFormat {
        match (self.format, self.depth) {
            (TexelFormat::Rgba, TexelDepth::Bit8) => RGBA8_FORMAT,
        }
    }

    pub fn shader_def(&self) -> &'static str {
        match (self.format, self.depth) {
            (TexelFormat::Rgba, TexelDepth::Bit8) => "RGBA8",
        }
    }

    pub fn sample_type(&self) -> TextureSampleType {
        match (self.format, self.depth) {
            (TexelFormat::Rgba, TexelDepth::Bit8) => TextureSampleType::Uint,
        }
    }

    pub fn convert_image_to_wgpu(&self, img: DynamicImage) -> Vec<u8> {
        match (self.format, self.depth) {
            (TexelFormat::Rgba, TexelDepth::Bit8) => {
                let rgba8 = img.to_rgba8();
                rgba8.into_raw()
            }
        }
    }
}
