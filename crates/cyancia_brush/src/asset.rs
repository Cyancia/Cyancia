use std::{
    collections::{HashMap, HashSet},
    io::{Cursor, Read},
    path::Path,
    sync::Arc,
};

use bevy_math::IRect;
use cyancia_assets::{
    asset::{Asset, AssetHandle, AssetId},
    loader::AssetSerializer,
    store::AssetRegistry,
};
use cyancia_shader_graph::{
    graph::{
        Graph, GraphCompileError, GraphDynamicInstancesStorage,
        node::external::{
            ExternalNode, ExternalVariable, ExternalVariableId, ExternalVariableStorage,
        },
        variable::GraphLiteral,
    },
    save::{
        GraphDeserializeError, GraphSerializable, SerializableExternalVariable, SerializableGraph,
        SerializableGraphLiteral,
    },
    wgsl_std::nodes::{TextureNode, TextureStorage, TextureUsageRecorder},
};
use glam::{IVec2, UVec2};
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

use crate::render::graph::{GraphInputParams, generate_brush_shader};

pub struct BrushPreset {
    pub metadata: BrushPresetMetadata,
    pub main_graph: SerializableGraph,
    pub external_vars: HashMap<ExternalVariableId, SerializableExternalVariable>,
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
        let mut external_vars_buffer = String::new();
        if let Ok(mut f) = archive.by_name("external_vars.toml") {
            f.read_to_string(&mut external_vars_buffer)?;
        }

        let external_vars = toml::from_str::<
            HashMap<ExternalVariableId, SerializableExternalVariable>,
        >(&external_vars_buffer)?;

        Ok(BrushPreset {
            metadata,
            main_graph,
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
    external_vars: Arc<ExternalVariableStorage>,

    main_graph_storage: Arc<GraphDynamicInstancesStorage>,
    texture_usage_recorder: Arc<TextureUsageRecorder>,
}

impl BrushPresetInstance {
    pub fn new(
        metadata: BrushPresetMetadata,
        mut main_graph_storage: GraphDynamicInstancesStorage,
        texture_storage: Arc<TextureStorage>,
    ) -> Self {
        let external_vars = Arc::new(ExternalVariableStorage::default());
        let texture_usage_recorder = Arc::new(TextureUsageRecorder::default());

        register_extra_nodes(
            &mut main_graph_storage,
            external_vars.clone(),
            texture_storage,
            texture_usage_recorder.clone(),
        );
        let main_graph_storage = Arc::new(main_graph_storage);

        Self {
            metadata,
            main_graph: Graph::new(main_graph_storage.clone()),
            main_graph_storage,
            texture_usage_recorder,
            external_vars: Arc::new(ExternalVariableStorage::default()),
        }
    }

    pub fn from_asset(
        preset: &BrushPreset,
        mut main_graph_storage: GraphDynamicInstancesStorage,
        texture_storage: Arc<TextureStorage>,
    ) -> (Option<Self>, Vec<GraphDeserializeError>) {
        let external_vars = preset
            .external_vars
            .iter()
            .map(|(id, var)| {
                // TODO Err handling
                (
                    id.clone(),
                    Arc::new(var.deserialize(&main_graph_storage).unwrap()),
                )
            })
            .collect::<HashMap<_, _>>();

        let external_vars = Arc::new(ExternalVariableStorage::from_hashmap(external_vars));
        let texture_usage_recorder = Arc::new(TextureUsageRecorder::default());
        register_extra_nodes(
            &mut main_graph_storage,
            external_vars.clone(),
            texture_storage.clone(),
            texture_usage_recorder.clone(),
        );

        let main_graph_storage = Arc::new(main_graph_storage);
        let mut errors = Vec::new();
        let main_graph = {
            let (g, e) = Graph::from_serialized(main_graph_storage.clone(), &preset.main_graph);
            errors.extend(e);
            match g {
                Some(g) => g,
                None => return (None, errors),
            }
        };

        (
            Some(Self {
                metadata: preset.metadata.clone(),
                main_graph,
                external_vars,
                main_graph_storage,
                texture_usage_recorder,
            }),
            errors,
        )
    }

    pub fn as_asset(&self) -> anyhow::Result<BrushPreset> {
        let main_graph = self.main_graph.as_serialized()?;
        let external_vars = self
            .external_vars
            .all()
            .iter()
            .map(|(id, value)| {
                Result::<_, toml::ser::Error>::Ok((
                    *id,
                    SerializableExternalVariable::serialize(value.as_ref())?,
                ))
            })
            .collect::<Result<HashMap<_, _>, _>>()?;

        Ok(BrushPreset {
            metadata: self.metadata.clone(),
            main_graph,
            external_vars,
        })
    }

    pub fn estimate_size(&self) -> UVec2 {
        // TODO
        UVec2::splat(16)
    }

    pub fn estimate_area(&self, params: &GraphInputParams) -> IRect {
        // TODO
        IRect::from_center_size(
            params.pen_position.as_ivec2(),
            self.estimate_size().as_ivec2(),
        )
    }

    pub fn compile(
        &mut self,
        output_count: u32,
    ) -> Result<(String, Arc<TextureUsageRecorder>), anyhow::Error> {
        self.texture_usage_recorder.reset();
        let shader = generate_brush_shader(&mut self.main_graph, output_count)?;
        Ok((shader, self.texture_usage_recorder.clone()))
    }

    pub fn main_graph(&self) -> &Graph {
        &self.main_graph
    }

    pub fn main_graph_mut(&mut self) -> &mut Graph {
        &mut self.main_graph
    }

    pub fn external_vars(&self) -> &Arc<ExternalVariableStorage> {
        &self.external_vars
    }

    pub fn metadata(&self) -> &BrushPresetMetadata {
        &self.metadata
    }
}

fn register_extra_nodes(
    storage: &mut GraphDynamicInstancesStorage,
    external_vars: Arc<ExternalVariableStorage>,
    texture_storage: Arc<TextureStorage>,
    texture_usage_recorder: Arc<TextureUsageRecorder>,
) {
    storage
        .nodes
        .register_non_default(ExternalNode::new(external_vars));
    storage
        .nodes
        .register_non_default(TextureNode::new(texture_storage, texture_usage_recorder));
}

#[derive(Debug, Clone)]
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
