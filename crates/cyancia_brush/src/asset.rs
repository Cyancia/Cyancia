use std::{
    collections::{HashMap, HashSet},
    io::{Cursor, Read},
    path::Path,
    sync::Arc,
};

use cyancia_assets::{
    asset::{Asset, AssetHandle, AssetId},
    loader::AssetSerializer,
    store::AssetRegistry,
};
use cyancia_shader_graph::{
    graph::{
        Graph, GraphCompileError, GraphDynamicInstancesStorage,
        node::external::{ExternalDataStorage, ExternalLiteralId, ExternalNode},
        variable::GraphLiteral,
    },
    save::{GraphDeserializeError, GraphSerializable, SerializableGraph, SerializableGraphLiteral},
    wgsl_std::types::{TextureReference, TextureType},
};
use glam::UVec2;
use image::{DynamicImage, ImageFormat};
use indexmap::{IndexMap, IndexSet};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use wgpu::{
    Device, Extent3d, Queue, Texture, TextureDimension, TextureFormat, TextureUsages,
    util::DeviceExt,
    wgt::{TextureDataOrder, TextureDescriptor},
};
use zip::ZipArchive;

use crate::render::graph::generate_brush_shader;

pub struct BrushPreset {
    pub metadata: BrushPresetMetadata,
    pub main_graph: SerializableGraph,
    pub textures: Vec<Image>,
    pub functions: Vec<SerializableGraph>,
    pub external_vars: HashMap<ExternalLiteralId, SerializableGraphLiteral>,
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
    Image(#[from] ImageSerializerError),
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
                Ok(ImageSerializer.read(&mut Cursor::new(data))?)
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
        let mut external_vars_buffer = String::new();
        if let Ok(mut f) = archive.by_name("external_vars.toml") {
            f.read_to_string(&mut external_vars_buffer)?;
        }

        let external_vars = toml::from_str::<HashMap<ExternalLiteralId, SerializableGraphLiteral>>(
            &external_vars_buffer,
        )?;

        Ok(BrushPreset {
            metadata,
            main_graph,
            textures,
            functions,
            external_vars,
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

pub struct BrushPresetInstance {
    metadata: BrushPresetMetadata,
    main_graph: Graph,
    functions: Vec<Graph>,
    external_vars: Arc<ExternalDataStorage>,
    referenced_textures: IndexSet<AssetHandle<Image>>,
    dirty_texture_variables: bool,
}

impl BrushPresetInstance {
    pub fn from_asset(
        preset: &BrushPreset,
        mut main_storage: GraphDynamicInstancesStorage,
        function_storage: GraphDynamicInstancesStorage,
        asset_registry: &AssetRegistry,
    ) -> (Option<Self>, Vec<GraphDeserializeError>) {
        let external_vars = preset
            .external_vars
            .iter()
            .map(|(id, var)| {
                // TODO Err handling
                (
                    id.clone(),
                    Arc::new(var.deserialize(&main_storage).unwrap()),
                )
            })
            .collect::<HashMap<_, _>>();

        let mut referenced_textures = IndexSet::new();
        for var in external_vars.values() {
            if let Some(texture_ref) = var.try_as_ref::<TextureReference>() {
                let Ok(handle) = asset_registry.handle(AssetId::new(texture_ref.global_id)) else {
                    continue;
                };
                referenced_textures.insert(handle);
            }
        }

        let external_vars = Arc::new(ExternalDataStorage::from_hashmap(external_vars));
        main_storage
            .nodes
            .register_non_default(ExternalNode::new(external_vars.clone()));

        let mut errors = Vec::new();
        let main_graph = {
            let (g, e) = Graph::from_serialized(Arc::new(main_storage), preset.main_graph.clone());
            errors.extend(e);
            match g {
                Some(g) => g,
                None => return (None, errors),
            }
        };

        let mut functions = Vec::with_capacity(preset.functions.len());
        for function in &preset.functions {
            let (f, e) =
                Graph::from_serialized(Arc::new(function_storage.clone()), function.clone());
            if let Some(f) = f {
                functions.push(f);
            }
            errors.extend(e);
        }

        (
            Some(Self {
                metadata: preset.metadata.clone(),
                main_graph,
                functions,
                external_vars,
                referenced_textures,
                dirty_texture_variables: false,
            }),
            errors,
        )
    }

    pub fn add_texture_reference(&mut self, asset: AssetHandle<Image>) {
        self.referenced_textures.insert(asset);
        self.dirty_texture_variables = true;
    }

    pub fn remove_texture_reference(&mut self, asset_id: &AssetHandle<Image>) {
        self.referenced_textures.swap_remove(asset_id);
        self.dirty_texture_variables = true;
    }

    pub fn estimate_size(&self) -> UVec2 {
        // TODO
        UVec2::splat(512)
    }

    pub fn compile(&mut self) -> Result<String, anyhow::Error> {
        if self.dirty_texture_variables {
            self.reset_external_texture_variables();
        }
        generate_brush_shader(&mut self.main_graph)
    }

    fn reset_external_texture_variables(&mut self) {
        let all = self.external_vars.all();
        let texture_refs = all
            .iter()
            .filter_map(|(id, var)| var.try_as_ref::<TextureReference>().map(|_| id))
            .collect::<Vec<_>>();

        for id in texture_refs {
            self.external_vars.remove(&id);
        }

        for (local_index, handle) in self.referenced_textures.iter().enumerate() {
            let Ok(asset) = handle.get() else {
                continue;
            };

            let reference = TextureReference {
                global_id: *handle.id(),
                local_index: local_index as u32,
            };
            self.external_vars.insert(
                ExternalLiteralId::new(asset.metadata.name.clone()),
                GraphLiteral::new::<TextureType>(reference),
            );
        }
    }

    pub fn main_graph(&self) -> &Graph {
        &self.main_graph
    }

    pub fn main_graph_mut(&mut self) -> &mut Graph {
        &mut self.main_graph
    }

    pub fn referenced_textures(&self) -> &IndexSet<AssetHandle<Image>> {
        &self.referenced_textures
    }

    pub fn external_vars(&self) -> &Arc<ExternalDataStorage> {
        &self.external_vars
    }
}

pub struct GpuImage {
    pub texture: Texture,
}

impl GpuImage {
    pub fn from_asset(device: &Device, queue: &Queue, asset: &Image) -> Self {
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
