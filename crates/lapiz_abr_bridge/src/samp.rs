use anyhow::Result;
use image::{DynamicImage, ImageFormat, RgbaImage};
use lapiz_abr::{Sample, SampleImage};
use lapiz_render::texture::{Image, ImageMetadata};

pub fn parse_samp(sample: &Sample) -> Result<Image> {
    let sample_image = sample.as_image()?;
    let (width, height, alpha) = match sample_image {
        SampleImage::Bit8(image) => {
            let (width, height) = image.dimensions();
            (width, height, image.into_raw())
        }
        SampleImage::Bit16(image) => {
            let (width, height) = image.dimensions();
            let alpha = image
                .into_raw()
                .into_iter()
                .map(|value| ((u32::from(value) * 255 + 32767) / 65535) as u8)
                .collect();
            (width, height, alpha)
        }
    };
    // TODO This is an unpleasant workaround. We are assuming all textures as srgb textures,
    //      but masks should be linear. Since alpha channel won't be affected, we are manually
    //      convert this mask here.
    let pixels = alpha
        .into_iter()
        .flat_map(|alpha| [255, 255, 255, alpha])
        .collect();
    let image = RgbaImage::from_raw(width, height, pixels)
        .ok_or_else(|| anyhow::anyhow!("failed to construct sample image"))?;

    Ok(Image {
        metadata: ImageMetadata {
            name: sample.id.to_string(),
        },
        image: DynamicImage::ImageRgba8(image),
        format: ImageFormat::Png,
    })
}
