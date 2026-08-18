use anyhow::Result;
use image::ImageFormat;
use lapiz_abr::Sample;
use lapiz_render::texture::{Image, ImageMetadata};

pub fn parse_samp(sample: &Sample) -> Result<Image> {
    Ok(Image {
        metadata: ImageMetadata {
            name: sample.id.to_string(),
        },
        image: sample.as_image()?.into_dynamic(),
        format: ImageFormat::Png,
    })
}
