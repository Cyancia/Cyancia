use std::io::{Cursor, Read, Write};

use bevy_math::IRect;
use cyancia_assets::{
    asset::{Asset, AssetHandle, AssetId},
    loader::AssetSerializer,
};
use cyancia_shader_graph::save::{
    GraphDeserializeError, GraphSerializable, SerializableExternalVariable, SerializableGraph,
    SerializableGraphLiteral,
};
use image::{DynamicImage, ImageFormat};
use serde::{Deserialize, Serialize};
use wgpu::{
    Device, Extent3d, Queue, Texture, TextureDimension, TextureFormat, TextureUsages,
    util::DeviceExt,
    wgt::{TextureDataOrder, TextureDescriptor},
};
use zip::{ZipArchive, ZipWriter, write::FileOptions};

pub struct BrushPreset {
    pub metadata: BrushPresetMetadata,
    pub spacing_factor_graph: SerializableGraph,
    pub required_spacing_graph: SerializableGraph,
    pub main_graph: SerializableGraph,
    pub stroke_postprocess_graphs: Vec<SerializableGraph>,
    pub external_vars: Vec<SerializableExternalVariable>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct BrushPresetMetadata {
    pub name: String,
}

impl Asset for BrushPreset {
    const TYPE_NAME: &'static str = "brush_preset";
}

#[derive(Default)]
pub struct BrushPresetSerializer;

#[derive(Debug, thiserror::Error)]
pub enum BrushPresetSerializerError {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Zip(#[from] zip::result::ZipError),
    #[error(transparent)]
    TomlDe(#[from] toml::de::Error),
    #[error(transparent)]
    TomlSer(#[from] toml::ser::Error),
    #[error(transparent)]
    Image(#[from] ImageSerializerError),
}

impl AssetSerializer for BrushPresetSerializer {
    type Asset = BrushPreset;

    type Error = BrushPresetSerializerError;

    fn file_extension() -> &'static str {
        "cbp"
    }

    // TODO: Final .cbp file definition.
    // TODO: Support embedded textures and shader graph functions.
    fn read(&self, reader: &mut dyn Read) -> Result<Self::Asset, Self::Error> {
        let mut buf = Vec::new();
        reader.read_to_end(&mut buf)?;
        let mut archive = ZipArchive::new(std::io::Cursor::new(buf))?;

        let mut main_graph_buffer = String::new();
        archive
            .by_name("main.csg")?
            .read_to_string(&mut main_graph_buffer)?;
        let main_graph = toml::from_str::<SerializableGraph>(&main_graph_buffer)?;
        let mut metadata_buffer = String::new();
        archive
            .by_name("metadata.toml")?
            .read_to_string(&mut metadata_buffer)?;
        let metadata = toml::from_str::<BrushPresetMetadata>(&metadata_buffer)?;

        let mut spacing_factor_graph_buffer = String::new();
        archive
            .by_name("spacing_factor.csg")?
            .read_to_string(&mut spacing_factor_graph_buffer)?;
        let spacing_factor_graph =
            toml::from_str::<SerializableGraph>(&spacing_factor_graph_buffer)?;
        let mut required_spacing_graph_buffer = String::new();
        archive
            .by_name("required_spacing.csg")?
            .read_to_string(&mut required_spacing_graph_buffer)?;
        let required_spacing_graph =
            toml::from_str::<SerializableGraph>(&required_spacing_graph_buffer)?;

        let external_vars = match archive.by_name("external_vars.toml") {
            Ok(mut f) => {
                let mut external_vars_buffer = String::new();
                f.read_to_string(&mut external_vars_buffer)?;
                external_vars_buffer
                    .parse::<toml::Value>()?
                    .try_into::<Vec<SerializableExternalVariable>>()?
            }
            Err(_) => Default::default(),
        };

        let mut stroke_postprocess_graphs = Vec::new();
        let files = archive
            .file_names()
            .map(ToString::to_string)
            .collect::<Vec<_>>();
        for file in files {
            if file.starts_with("stroke_postprocess/") && file != "stroke_postprocess/" {
                let mut buf = String::new();
                archive.by_name(&file)?.read_to_string(&mut buf)?;
                let graph = toml::from_str::<SerializableGraph>(&buf)?;
                stroke_postprocess_graphs.push(graph);
            }
        }

        Ok(BrushPreset {
            metadata,
            required_spacing_graph,
            spacing_factor_graph,
            main_graph,
            stroke_postprocess_graphs,
            external_vars,
        })
    }

    fn write(
        &self,
        asset: &Self::Asset,
        writer: &mut dyn std::io::Write,
    ) -> Result<(), Self::Error> {
        let mut buf = Vec::new();
        let mut zip = ZipWriter::new(Cursor::new(&mut buf));

        zip.start_file("spacing_factor.csg", FileOptions::<()>::default())?;
        let spacing_factor_graph_buffer = toml::to_string(&asset.spacing_factor_graph)?;
        zip.write_all(spacing_factor_graph_buffer.as_bytes())?;

        zip.start_file("required_spacing.csg", FileOptions::<()>::default())?;
        let required_spacing_graph_buffer = toml::to_string(&asset.required_spacing_graph)?;
        zip.write_all(required_spacing_graph_buffer.as_bytes())?;

        zip.start_file("main.csg", FileOptions::<()>::default())?;
        let main_graph_buffer = toml::to_string(&asset.main_graph)?;
        zip.write_all(main_graph_buffer.as_bytes())?;

        zip.start_file("metadata.toml", FileOptions::<()>::default())?;
        let metadata_buffer = toml::to_string(&asset.metadata)?;
        zip.write_all(metadata_buffer.as_bytes())?;

        if !asset.external_vars.is_empty() {
            zip.start_file("external_vars.toml", FileOptions::<()>::default())?;
            let external_vars_buffer = toml::Value::try_from(&asset.external_vars)?.to_string();
            zip.write_all(external_vars_buffer.as_bytes())?;
        }

        for (i, graph) in asset.stroke_postprocess_graphs.iter().enumerate() {
            zip.start_file(
                format!("stroke_postprocess/{}.csg", i),
                FileOptions::<()>::default(),
            )?;
            let graph_buffer = toml::to_string(graph)?;
            zip.write_all(graph_buffer.as_bytes())?;
        }

        zip.finish()?;
        writer.write_all(&buf)?;

        Ok(())
    }
}

pub struct Image {
    pub metadata: ImageMetadata,
    pub image: DynamicImage,
    pub format: ImageFormat,
}

#[derive(Serialize, Deserialize)]
pub struct ImageMetadata {
    pub name: String,
}

impl Asset for Image {
    const TYPE_NAME: &'static str = "texture";
}

impl Image {
    pub fn from_buffer(metadata: ImageMetadata, buffer: &[u8]) -> Result<Self, image::ImageError> {
        let format = image::guess_format(buffer)?;
        let image = image::load_from_memory_with_format(buffer, format)?;
        Ok(Self {
            metadata,
            image,
            format,
        })
    }
}

#[derive(Default)]
pub struct ImageSerializer;

#[derive(Debug, thiserror::Error)]
pub enum ImageSerializerError {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Image(#[from] image::ImageError),
    #[error(transparent)]
    Zip(#[from] zip::result::ZipError),
    #[error(transparent)]
    Toml(#[from] toml::de::Error),
}

impl AssetSerializer for ImageSerializer {
    type Asset = Image;

    type Error = ImageSerializerError;

    fn file_extension() -> &'static str {
        "cig"
    }

    fn read(&self, reader: &mut dyn Read) -> Result<Self::Asset, Self::Error> {
        let mut buf = Vec::new();
        reader.read_to_end(&mut buf)?;
        let mut archive = ZipArchive::new(Cursor::new(buf))?;

        let metadata = {
            let mut buf = Vec::new();
            archive.by_name("metadata.toml")?.read_to_end(&mut buf)?;
            toml::from_slice::<ImageMetadata>(&buf)?
        };

        let (format, image) = {
            let mut buf = Vec::new();
            archive.by_name("image")?.read_to_end(&mut buf)?;
            let format = image::guess_format(&buf)?;
            let image = image::load_from_memory_with_format(&buf, format)?;
            (format, image)
        };

        Ok(Image {
            metadata,
            image,
            format,
        })
    }

    fn write(
        &self,
        asset: &Self::Asset,
        writer: &mut dyn std::io::Write,
    ) -> Result<(), Self::Error> {
        todo!()
    }
}

#[derive(Debug, Clone)]
pub struct GpuImage {
    pub texture: Texture,
}

impl GpuImage {
    pub fn from_asset(device: &Device, queue: &Queue, asset: &Image, usage: TextureUsages) -> Self {
        use bytemuck::cast_slice;
        let width;
        let height;

        let data: Vec<u8>;
        let format: TextureFormat;
        // TODO: This can be incorrect sometimes, but mostly we are loading srgb textures.
        let is_srgb = true;
        let dyn_img = asset.image.clone();

        // Copied from Bevy project
        match dyn_img {
            DynamicImage::ImageLuma8(image) => {
                let i = DynamicImage::ImageLuma8(image).into_rgba8();
                width = i.width();
                height = i.height();
                format = if is_srgb {
                    TextureFormat::Rgba8UnormSrgb
                } else {
                    TextureFormat::Rgba8Unorm
                };

                data = i.into_raw();
            }
            DynamicImage::ImageLumaA8(image) => {
                let i = DynamicImage::ImageLumaA8(image).into_rgba8();
                width = i.width();
                height = i.height();
                format = if is_srgb {
                    TextureFormat::Rgba8UnormSrgb
                } else {
                    TextureFormat::Rgba8Unorm
                };

                data = i.into_raw();
            }
            DynamicImage::ImageRgb8(image) => {
                let i = DynamicImage::ImageRgb8(image).into_rgba8();
                width = i.width();
                height = i.height();
                format = if is_srgb {
                    TextureFormat::Rgba8UnormSrgb
                } else {
                    TextureFormat::Rgba8Unorm
                };

                data = i.into_raw();
            }
            DynamicImage::ImageRgba8(image) => {
                width = image.width();
                height = image.height();
                format = if is_srgb {
                    TextureFormat::Rgba8UnormSrgb
                } else {
                    TextureFormat::Rgba8Unorm
                };

                data = image.into_raw();
            }
            DynamicImage::ImageLuma16(image) => {
                width = image.width();
                height = image.height();
                format = TextureFormat::R16Uint;

                let raw_data = image.into_raw();

                data = cast_slice(&raw_data).to_owned();
            }
            DynamicImage::ImageLumaA16(image) => {
                width = image.width();
                height = image.height();
                format = TextureFormat::Rg16Uint;

                let raw_data = image.into_raw();

                data = cast_slice(&raw_data).to_owned();
            }
            DynamicImage::ImageRgb16(image) => {
                let i = DynamicImage::ImageRgb16(image).into_rgba16();
                width = i.width();
                height = i.height();
                format = TextureFormat::Rgba16Unorm;

                let raw_data = i.into_raw();

                data = cast_slice(&raw_data).to_owned();
            }
            DynamicImage::ImageRgba16(image) => {
                width = image.width();
                height = image.height();
                format = TextureFormat::Rgba16Unorm;

                let raw_data = image.into_raw();

                data = cast_slice(&raw_data).to_owned();
            }
            DynamicImage::ImageRgb32F(image) => {
                width = image.width();
                height = image.height();
                format = TextureFormat::Rgba32Float;
                let pixel_size = format.block_copy_size(None).unwrap() as usize;

                let mut local_data =
                    Vec::with_capacity(width as usize * height as usize * pixel_size);

                for pixel in image.into_raw().chunks_exact(3) {
                    // TODO: use the array_chunks method once stabilized
                    // https://github.com/rust-lang/rust/issues/74985
                    let r = pixel[0];
                    let g = pixel[1];
                    let b = pixel[2];
                    let a = 1f32;

                    local_data.extend_from_slice(&r.to_le_bytes());
                    local_data.extend_from_slice(&g.to_le_bytes());
                    local_data.extend_from_slice(&b.to_le_bytes());
                    local_data.extend_from_slice(&a.to_le_bytes());
                }

                data = local_data;
            }
            DynamicImage::ImageRgba32F(image) => {
                width = image.width();
                height = image.height();
                format = TextureFormat::Rgba32Float;

                let raw_data = image.into_raw();

                data = cast_slice(&raw_data).to_owned();
            }
            // DynamicImage is now non exhaustive, catch future variants and convert them
            _ => {
                let image = dyn_img.into_rgba8();
                width = image.width();
                height = image.height();
                format = TextureFormat::Rgba8UnormSrgb;

                data = image.into_raw();
            }
        }

        let texture = device.create_texture_with_data(
            queue,
            &TextureDescriptor {
                label: None,
                size: Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: TextureDimension::D2,
                format,
                usage: TextureUsages::COPY_DST | usage,
                view_formats: &[],
            },
            TextureDataOrder::default(),
            &data,
        );

        Self { texture }
    }
}
