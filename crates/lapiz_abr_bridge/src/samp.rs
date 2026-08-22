use anyhow::Result;
use image::{DynamicImage, GrayImage, ImageFormat};
use lapiz_abr::{Sample, SampleImage};
use lapiz_render::texture::{Image, ImageMetadata};

pub fn parse_samp(sample: &Sample) -> Result<Image> {
    let sample_image = sample.as_image()?;
    let (width, height, pixels) = match sample_image {
        SampleImage::Bit8(image) => {
            let (width, height) = image.dimensions();
            let pixels = image
                .into_raw()
                .into_iter()
                .map(|value| encode_sample_mask(f32::from(value) / 255.0))
                .collect();
            (width, height, pixels)
        }
        SampleImage::Bit16(image) => {
            let (width, height) = image.dimensions();
            let pixels = image
                .into_raw()
                .into_iter()
                .map(|value| encode_sample_mask(f32::from(value) / 65535.0))
                .collect();
            (width, height, pixels)
        }
    };
    let image = GrayImage::from_raw(width, height, pixels)
        .ok_or_else(|| anyhow::anyhow!("failed to construct converted sample image"))?;

    Ok(Image {
        metadata: ImageMetadata {
            name: sample.id.to_string(),
        },
        image: DynamicImage::ImageLuma8(image),
        format: ImageFormat::Png,
    })
}

fn encode_sample_mask(value: f32) -> u8 {
    let mask = 1.0 - linear_to_srgb(1.0 - value);
    (linear_to_srgb(mask) * 255.0).round() as u8
}

fn linear_to_srgb(value: f32) -> f32 {
    if value <= 0.0031308 {
        value * 12.92
    } else {
        1.055 * value.powf(1.0 / 2.4) - 0.055
    }
}
