use anyhow::Result;
use image::ImageFormat;
use lapiz_abr::Pattern;
use lapiz_render::texture::{Image, ImageMetadata};

pub fn parse_patt(pattern: &Pattern) -> Result<Image> {
    Ok(Image {
        metadata: ImageMetadata {
            name: pattern.name.clone(),
        },
        image: pattern.as_image()?,
        format: ImageFormat::Png,
    })
}
