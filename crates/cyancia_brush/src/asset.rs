use std::{
    collections::{HashMap, HashSet},
    fmt::Display,
    io::{Cursor, Read, Write},
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
        Graph, GraphCompileError, GraphDynamicInstancesStorage, GraphFunctionStorage,
        node::{
            external::{
                ExternalNode, ExternalVariable, ExternalVariableId, ExternalVariableStorage,
                generate_external_variable_binding,
            },
            function::GraphFunctionNode,
        },
        variable::GraphLiteral,
    },
    save::{
        GraphDeserializeError, GraphSerializable, SerializableExternalVariable, SerializableGraph,
        SerializableGraphLiteral,
    },
    wgsl_std::{
        nodes::{TextureId, TextureNode, TextureStorage, TextureUsageRecorder},
        std_storage,
    },
};
use glam::{IVec2, UVec2};
use image::{DynamicImage, ImageFormat};
use indexmap::{IndexMap, IndexSet};
use parking_lot::{RwLock, RwLockReadGuard, RwLockWriteGuard};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use wesl::{VirtualResolver, Wesl};
use wgpu::{
    Device, Extent3d, Queue, Texture, TextureDimension, TextureFormat, TextureUsages,
    util::DeviceExt,
    wgt::{TextureDataOrder, TextureDescriptor},
};
use zip::{ZipArchive, ZipWriter, write::FileOptions};

use crate::render::graph::{GraphInputParams, brush_graph_storage};

pub struct BrushPreset {
    pub metadata: BrushPresetMetadata,
    pub main_graph: SerializableGraph,
    pub stroke_postprocess_graphs: Vec<SerializableGraph>,
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
        let mut external_vars_buffer = String::new();
        if let Ok(mut f) = archive.by_name("external_vars.toml") {
            f.read_to_string(&mut external_vars_buffer)?;
        }

        let mut stroke_postprocess_graphs = Vec::new();
        let files = archive
            .file_names()
            .map(ToString::to_string)
            .collect::<Vec<_>>();
        for file in files {
            if file.starts_with("stroke_postprocess") {
                let mut buf = String::new();
                archive.by_name(&file)?.read_to_string(&mut buf)?;
                let graph = toml::from_str::<SerializableGraph>(&buf)?;
                stroke_postprocess_graphs.push(graph);
            }
        }

        let external_vars = toml::from_str::<
            HashMap<ExternalVariableId, SerializableExternalVariable>,
        >(&external_vars_buffer)?;

        Ok(BrushPreset {
            metadata,
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

        zip.start_file("main.csg", FileOptions::<()>::default())?;
        let main_graph_buffer = toml::to_string(&asset.main_graph)?;
        zip.write_all(main_graph_buffer.as_bytes())?;

        zip.start_file("metadata.toml", FileOptions::<()>::default())?;
        let metadata_buffer = toml::to_string(&asset.metadata)?;
        zip.write_all(metadata_buffer.as_bytes())?;

        zip.start_file("external_vars.toml", FileOptions::<()>::default())?;
        let external_vars_buffer = toml::to_string(&asset.external_vars)?;
        zip.write_all(external_vars_buffer.as_bytes())?;

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

pub struct CompiledBrushPreset {
    pub main_graph: String,
    pub stroke_postprocess_graphs: Vec<String>,
    pub texture_usages: Vec<TextureId>,
}

impl Display for CompiledBrushPreset {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "-------------- Compiled brush preset --------------")?;
        writeln!(
            f,
            "-------------- Main graph shader -------------- \n{}",
            self.main_graph
        )?;
        for (i, graph) in self.stroke_postprocess_graphs.iter().enumerate() {
            writeln!(
                f,
                "-------------- Stroke postprocess graph {} shader -------------- \n{}",
                i, graph
            )?;
        }
        writeln!(f, "-------------- Texture usages --------------")?;
        for usage in &self.texture_usages {
            writeln!(f, "  - {}", usage)?;
        }
        Ok(())
    }
}

pub struct BrushPresetInstance {
    metadata: RwLock<BrushPresetMetadata>,

    main_graph: Arc<RwLock<Graph>>,
    stroke_postprocess_graphs: RwLock<Vec<Arc<RwLock<Graph>>>>,

    external_vars: Arc<ExternalVariableStorage>,

    main_graph_storage: Arc<GraphDynamicInstancesStorage>,
    postprocess_graph_storage: Arc<GraphDynamicInstancesStorage>,
    texture_usage_recorder: Arc<TextureUsageRecorder>,
}

impl BrushPresetInstance {
    pub fn new(
        metadata: BrushPresetMetadata,
        texture_storage: Arc<TextureStorage>,
        function_storage: Arc<GraphFunctionStorage>,
    ) -> Self {
        let external_vars = Arc::new(ExternalVariableStorage::default());
        let texture_usage_recorder = Arc::new(TextureUsageRecorder::default());

        let main_graph_storage = Arc::new(create_main_graph_storage(
            external_vars.clone(),
            texture_storage.clone(),
            texture_usage_recorder.clone(),
            function_storage.clone(),
        ));
        let postprocess_graph_storage = Arc::new(create_postprocess_graph_storage(
            external_vars.clone(),
            texture_storage.clone(),
            texture_usage_recorder.clone(),
            function_storage.clone(),
        ));

        Self {
            metadata: RwLock::new(metadata),
            main_graph: Arc::new(RwLock::new(Graph::new(main_graph_storage.clone()))),
            stroke_postprocess_graphs: RwLock::new(Vec::new()),
            main_graph_storage,
            postprocess_graph_storage,
            external_vars: Arc::new(ExternalVariableStorage::default()),
            texture_usage_recorder,
        }
    }

    pub fn from_asset(
        preset: &BrushPreset,
        texture_storage: Arc<TextureStorage>,
        function_storage: Arc<GraphFunctionStorage>,
    ) -> (Option<Self>, Vec<GraphDeserializeError>) {
        let texture_usage_recorder = Arc::new(TextureUsageRecorder::default());

        let main_graph_storage = Arc::new(create_main_graph_storage(
            Default::default(),
            texture_storage.clone(),
            texture_usage_recorder.clone(),
            function_storage.clone(),
        ));

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
        let main_graph_storage = Arc::new(create_main_graph_storage(
            external_vars.clone(),
            texture_storage.clone(),
            texture_usage_recorder.clone(),
            function_storage.clone(),
        ));
        let postprocess_graph_storage = Arc::new(create_postprocess_graph_storage(
            external_vars.clone(),
            texture_storage.clone(),
            texture_usage_recorder.clone(),
            function_storage.clone(),
        ));

        let mut errors = Vec::new();
        let main_graph = {
            let (g, e) = Graph::from_serialized(main_graph_storage.clone(), &preset.main_graph);
            errors.extend(e);
            match g {
                Some(g) => g,
                None => return (None, errors),
            }
        };

        let mut stroke_postprocess_graphs =
            Vec::with_capacity(preset.stroke_postprocess_graphs.len());
        for serialized in &preset.stroke_postprocess_graphs {
            let (g, e) = Graph::from_serialized(postprocess_graph_storage.clone(), serialized);
            errors.extend(e);
            match g {
                Some(g) => stroke_postprocess_graphs.push(Arc::new(RwLock::new(g))),
                None => return (None, errors),
            }
        }

        (
            Some(Self {
                metadata: RwLock::new(preset.metadata.clone()),
                main_graph: Arc::new(RwLock::new(main_graph)),
                stroke_postprocess_graphs: RwLock::new(stroke_postprocess_graphs),
                external_vars,
                main_graph_storage,
                postprocess_graph_storage,
                texture_usage_recorder,
            }),
            errors,
        )
    }

    pub fn as_asset(&self) -> anyhow::Result<BrushPreset> {
        let main_graph = self.main_graph.read().as_serialized()?;
        let stroke_postprocess_graphs = self
            .stroke_postprocess_graphs
            .read()
            .iter()
            .map(|g| g.read().as_serialized())
            .collect::<anyhow::Result<Vec<_>>>()?;
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
            metadata: self.metadata.read().clone(),
            main_graph,
            stroke_postprocess_graphs,
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

    pub fn compile(&self) -> Result<CompiledBrushPreset, anyhow::Error> {
        self.texture_usage_recorder.reset();

        let mut external_variable_bindings = String::new();
        let mut cur_binding = 4;
        for var in self.external_vars.all().values() {
            external_variable_bindings
                .extend(generate_external_variable_binding(0, cur_binding, var.as_ref()).chars());
            cur_binding += 1;
        }

        let main_graph = compile(&mut self.main_graph.write(), &external_variable_bindings)?;
        let stroke_postprocess_graphs = self.stroke_postprocess_graphs.write();
        let mut compiled_stroke_postprocess_graphs =
            Vec::with_capacity(stroke_postprocess_graphs.len());
        for graph in stroke_postprocess_graphs.iter() {
            compiled_stroke_postprocess_graphs
                .push(compile(&mut graph.write(), &external_variable_bindings)?);
        }

        Ok(CompiledBrushPreset {
            main_graph,
            stroke_postprocess_graphs: compiled_stroke_postprocess_graphs,
            texture_usages: self
                .texture_usage_recorder
                .get_usage()
                .keys()
                .cloned()
                .collect(),
        })
    }

    pub fn main_graph(&self) -> Arc<RwLock<Graph>> {
        self.main_graph.clone()
    }

    pub fn main_graph_read(&self) -> RwLockReadGuard<'_, Graph> {
        self.main_graph.read()
    }

    pub fn main_graph_mut(&self) -> RwLockWriteGuard<'_, Graph> {
        self.main_graph.write()
    }

    pub fn stroke_postprocess_graphs(&self) -> RwLockReadGuard<'_, Vec<Arc<RwLock<Graph>>>> {
        self.stroke_postprocess_graphs.read()
    }

    pub fn stroke_postprocess_graphs_mut(&self) -> RwLockWriteGuard<'_, Vec<Arc<RwLock<Graph>>>> {
        self.stroke_postprocess_graphs.write()
    }

    pub fn stroke_postprocess_graph(&self, index: usize) -> Option<Arc<RwLock<Graph>>> {
        self.stroke_postprocess_graphs.read().get(index).cloned()
    }

    pub fn new_stroke_postprocess_graph(&self) {
        self.stroke_postprocess_graphs
            .write()
            .push(Arc::new(RwLock::new(Graph::new(
                self.postprocess_graph_storage.clone(),
            ))));
    }

    pub fn external_vars(&self) -> &Arc<ExternalVariableStorage> {
        &self.external_vars
    }

    pub fn metadata(&self) -> RwLockReadGuard<'_, BrushPresetMetadata> {
        self.metadata.read()
    }

    pub fn metadata_mut(&self) -> RwLockWriteGuard<'_, BrushPresetMetadata> {
        self.metadata.write()
    }
}

fn compile(graph: &mut Graph, external_variable_bindings: &str) -> anyhow::Result<String> {
    let template = include_str!("render/brush_template.wesl");
    let (_, shader) = graph.compile(Vec::new(), Default::default())?;
    let shader = template
        .replace("//CODEGENFLAG_COMPILED_GRAPH", &shader)
        .replace(
            "//CODEGENFLAG_EXTERNAL_VARIABLE_BINDINGS",
            external_variable_bindings,
        );
    println!("Generated shader code:\n{}", shader);

    let mut resolver = VirtualResolver::new();
    resolver.add_module("template".parse().unwrap(), shader.into());
    resolver.add_module(
        "template/image::texture_unpack".parse().unwrap(),
        include_str!("../../cyancia_image/src/shaders/texture_unpack.wesl").into(),
    );
    resolver.add_module(
        "template/image::blend_modes".parse().unwrap(),
        include_str!("../../cyancia_image/src/shaders/blend_modes.wesl").into(),
    );
    let mut compiler = Wesl::new_barebones().set_custom_resolver(resolver);
    compiler.set_mangler(Default::default());
    compiler.set_options(Default::default());

    let shader = compiler.compile(&"template".parse().unwrap())?;
    Ok(shader.to_string())
}

fn create_main_graph_storage(
    external_vars: Arc<ExternalVariableStorage>,
    texture_storage: Arc<TextureStorage>,
    texture_usage_recorder: Arc<TextureUsageRecorder>,
    function_storage: Arc<GraphFunctionStorage>,
) -> GraphDynamicInstancesStorage {
    let mut storage = GraphDynamicInstancesStorage::default();
    storage.merge(std_storage());
    storage.merge(brush_graph_storage());
    storage
        .nodes
        .register_non_default(GraphFunctionNode::new(function_storage.clone()));
    storage
        .nodes
        .register_non_default(ExternalNode::new(external_vars));
    storage
        .nodes
        .register_non_default(TextureNode::new(texture_storage, texture_usage_recorder));
    storage
}

fn create_postprocess_graph_storage(
    external_vars: Arc<ExternalVariableStorage>,
    texture_storage: Arc<TextureStorage>,
    texture_usage_recorder: Arc<TextureUsageRecorder>,
    function_storage: Arc<GraphFunctionStorage>,
) -> GraphDynamicInstancesStorage {
    let mut storage = GraphDynamicInstancesStorage::default();
    storage.merge(std_storage());
    storage
        .nodes
        .register_non_default(GraphFunctionNode::new(function_storage.clone()));
    storage
        .nodes
        .register_non_default(ExternalNode::new(external_vars));
    storage
        .nodes
        .register_non_default(TextureNode::new(texture_storage, texture_usage_recorder));
    storage
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
