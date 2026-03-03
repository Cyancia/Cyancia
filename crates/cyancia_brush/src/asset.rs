use std::{io::Read, path::Path, sync::Arc};

use cyancia_assets::{asset::Asset, loader::AssetSerializer};
use cyancia_shader_graph::{
    graph::{Graph, GraphDynamicInstancesStorage},
    save::{GraphDeserializeError, GraphSerializable, SerializableGraph},
};
use image::DynamicImage;
use serde::{Deserialize, Serialize};
use wgpu::{
    Device, Extent3d, Queue, Texture, TextureDimension, TextureFormat, TextureUsages,
    util::DeviceExt,
    wgt::{TextureDataOrder, TextureDescriptor},
};
use zip::ZipArchive;

pub struct BrushPreset {
    pub metadata: BrushPresetMetadata,
    pub main_graph: SerializableGraph,
    pub textures: Vec<TextureResource>,
    pub functions: Vec<SerializableGraph>,
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
    Toml(#[from] toml::de::Error),
    #[error(transparent)]
    Image(#[from] image::ImageError),
}

impl AssetSerializer for BrushPresetSerializer {
    type Asset = BrushPreset;

    type Error = BrushPresetSerializerError;

    fn file_extension() -> &'static str {
        "cbp"
    }

    // TODO: Final .cbp file definition.
    fn read(&self, reader: &mut dyn Read) -> Result<Self::Asset, Self::Error> {
        let mut buf = Vec::new();
        reader.read_to_end(&mut buf)?;
        let mut archive = ZipArchive::new(std::io::Cursor::new(buf))?;

        let mut textures = Vec::new();
        let mut functions = Vec::new();

        for path_str in archive.file_names() {
            let path = Path::new(path_str);
            if path.starts_with("textures") && path.is_file() {
                textures.push(path_str.to_string());
            }
            if path.starts_with("functions") && path.is_file() {
                functions.push(path_str.to_string());
            }
        }

        let textures = textures
            .into_iter()
            .map(|path| {
                let mut data = Vec::new();
                archive.by_name(&path)?.read_to_end(&mut data)?;
                Ok(TextureResource {
                    image: image::load_from_memory(&data)?,
                })
            })
            .collect::<Result<Vec<_>, Self::Error>>()?;

        let functions = functions
            .into_iter()
            .map(|path| {
                let mut buffer = String::new();
                archive.by_name(&path)?.read_to_string(&mut buffer)?;
                let graph = toml::from_str::<SerializableGraph>(&buffer)?;
                Ok(graph)
            })
            .collect::<Result<Vec<_>, Self::Error>>()?;

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

        Ok(BrushPreset {
            metadata,
            main_graph,
            textures,
            functions,
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

pub struct TextureResource {
    pub image: DynamicImage,
}

pub struct BrushPresetInstance {
    pub metadata: BrushPresetMetadata,
    pub main_graph: Graph,
    pub textures: Vec<BrushGpuTexture>,
    pub functions: Vec<Graph>,
}

impl BrushPresetInstance {
    pub fn from_asset(
        preset: &BrushPreset,
        main_storage: Arc<GraphDynamicInstancesStorage>,
        function_storage: Arc<GraphDynamicInstancesStorage>,
        device: &Device,
        queue: &Queue,
    ) -> (Option<Self>, Vec<GraphDeserializeError>) {
        let mut errors = Vec::new();
        let main_graph = {
            let (g, e) = Graph::from_serialized(main_storage, preset.main_graph.clone());
            errors.extend(e);
            match g {
                Some(g) => g,
                None => return (None, errors),
            }
        };

        let mut functions = Vec::with_capacity(preset.functions.len());
        for function in &preset.functions {
            let (f, e) = Graph::from_serialized(function_storage.clone(), function.clone());
            if let Some(f) = f {
                functions.push(f);
            }
            errors.extend(e);
        }

        let textures = preset
            .textures
            .iter()
            .map(|tex| BrushGpuTexture::from_asset(device, queue, tex))
            .collect();

        (
            Some(Self {
                metadata: preset.metadata.clone(),
                main_graph,
                textures,
                functions,
            }),
            errors,
        )
    }
}

pub struct BrushGpuTexture {
    pub texture: Texture,
}

impl BrushGpuTexture {
    pub fn from_asset(device: &Device, queue: &Queue, asset: &TextureResource) -> Self {
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
                usage: TextureUsages::COPY_DST
                    | TextureUsages::TEXTURE_BINDING
                    | TextureUsages::COPY_SRC
                    | TextureUsages::STORAGE_BINDING,
                view_formats: &[],
            },
            TextureDataOrder::default(),
            &data,
        );

        Self { texture }
    }
}
