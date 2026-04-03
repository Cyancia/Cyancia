use std::{
    collections::{HashMap, HashSet},
    fmt::Display,
    io::{Cursor, Read, Write},
    path::Path,
    sync::{Arc, LazyLock},
};

use bevy_math::IRect;
use cyancia_assets::{
    asset::{Asset, AssetHandle, AssetId},
    loader::AssetSerializer,
    store::AssetRegistry,
};
use cyancia_shader_graph::{
    graph::{
        Graph, GraphCompileError, GraphResources,
        external::{
            ExternalVariable, ExternalVariableId, GraphExternalVariableStorage,
            generate_external_variable_binding,
        },
        function::GraphFunctionStorage,
        node::GraphNodeRegistry,
        slot::ErasedGraphLiteralUpdateMessage,
        texture::{GraphTextureStorage, GraphTextureUsageRecorder, TextureId},
        variable::{GraphLiteral, GraphTypeRegistry},
    },
    save::{
        GraphDeserializeError, GraphSerializable, SerializableExternalVariable, SerializableGraph,
        SerializableGraphLiteral,
    },
    wgsl_std::{builtin_nodes, builtin_types},
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

use crate::render::graph::{
    BlendColorNode, BlendWithInputNode, BlendWithLayerNode, CurrentPixelColorNode,
    DrawDirectionNode, EllipticalMaskNode, FilterWithinBoundsNode, FilterWithinMaskNode,
    GraphInputParams, LayerPixelColorNode, OutputColorNode, PasteTextureNode, PenPositionNode,
    PixelPositionNode, StrokeBoundsNode,
};

pub struct BrushPreset {
    pub metadata: BrushPresetMetadata,
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

pub struct CompiledBrushGraph {
    pub main: String,
    pub size_estimation: String,
}

impl Display for CompiledBrushGraph {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "-------------- Shader --------------")?;
        writeln!(f, "{}", self.main)?;
        writeln!(f, "-------------- Size estimation --------------")?;
        writeln!(f, "{}", self.size_estimation)?;
        Ok(())
    }
}

pub struct CompiledBrushPreset {
    pub input_sampling: String,
    pub main_graph: CompiledBrushGraph,
    pub stroke_postprocess_graphs: CompiledBrushGraph,
    pub n_stroke_postprocess_graphs: u32,
    pub texture_usage: Vec<TextureId>,
    pub external_vars: Arc<GraphExternalVariableStorage>,
}

impl Display for CompiledBrushPreset {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "-------------- Compiled brush preset --------------")?;
        writeln!(
            f,
            "-------------- Input sampling shader -------------- \n{}",
            self.input_sampling
        )?;
        writeln!(
            f,
            "-------------- Main graph shader -------------- \n{}",
            self.main_graph
        )?;
        writeln!(
            f,
            "-------------- Stroke postprocess graph shader -------------- \n{}",
            self.stroke_postprocess_graphs
        )?;
        writeln!(f, "-------------- Texture usages --------------")?;
        for usage in &self.texture_usage {
            writeln!(f, "  - {}", usage)?;
        }
        Ok(())
    }
}

pub struct BrushPresetInstance {
    brush_id: AssetId<BrushPreset>,
    metadata: BrushPresetMetadata,

    main_graph: Graph,
    stroke_postprocess_graphs: Vec<Graph>,
    graph_resources: GraphResources,
    is_dirty: bool,
}

impl BrushPresetInstance {
    pub fn from_asset(
        handle: &AssetHandle<BrushPreset>,
        textures: Arc<GraphTextureStorage>,
        functions: Arc<GraphFunctionStorage>,
    ) -> (Option<Self>, Vec<GraphDeserializeError>) {
        let preset = handle.get().unwrap();

        let external_vars = preset
            .external_vars
            .iter()
            .filter_map(|var| {
                var.deserialize(MAIN_GRAPH_TYPES.as_ref())
                    .inspect_err(|err| {
                        log::error!(
                            "Error deserializing external variable '{}': {}",
                            var.name,
                            err
                        );
                    })
                    .ok()
            })
            .collect::<Vec<_>>();
        let external_vars = Arc::new(GraphExternalVariableStorage::new(external_vars));
        let resources = GraphResources {
            textures,
            functions,
            external_vars: external_vars.clone(),
        };

        let mut errors = Vec::new();
        let main_graph = {
            let (g, e) = Graph::from_serialized(
                &preset.main_graph,
                resources.clone(),
                MAIN_GRAPH_TYPES.clone(),
                MAIN_GRAPH_NODES.as_ref(),
            );
            errors.extend(e);
            match g {
                Some(g) => g,
                None => return (None, errors),
            }
        };

        let mut stroke_postprocess_graphs =
            Vec::with_capacity(preset.stroke_postprocess_graphs.len());
        for serialized in &preset.stroke_postprocess_graphs {
            let (g, e) = Graph::from_serialized(
                serialized,
                resources.clone(),
                STROKE_POSTPROCESS_GRAPH_TYPES.clone(),
                STROKE_POSTPROCESS_GRAPH_NODES.as_ref(),
            );
            errors.extend(e);
            match g {
                Some(g) => stroke_postprocess_graphs.push(g),
                None => return (None, errors),
            }
        }

        (
            Some(Self {
                brush_id: handle.id(),
                metadata: preset.metadata.clone(),
                main_graph,
                stroke_postprocess_graphs,
                graph_resources: resources,
                is_dirty: true,
            }),
            errors,
        )
    }

    pub fn as_asset(&self) -> anyhow::Result<BrushPreset> {
        let main_graph = self.main_graph.as_serialized()?;
        let stroke_postprocess_graphs = self
            .stroke_postprocess_graphs
            .iter()
            .map(|g| g.as_serialized())
            .collect::<anyhow::Result<Vec<_>>>()?;
        let external_vars = self
            .graph_resources
            .external_vars
            .all()
            .iter()
            .map(|entry| SerializableExternalVariable::serialize(entry.value()))
            .collect::<Result<Vec<_>, _>>()?;

        Ok(BrushPreset {
            metadata: self.metadata.clone(),
            main_graph,
            stroke_postprocess_graphs,
            external_vars,
        })
    }

    pub fn compile(&self, mut existing_binding_count: u32) -> anyhow::Result<CompiledBrushPreset> {
        let mut external_variable_bindings = String::new();
        for entry in self.graph_resources.external_vars.all().iter() {
            external_variable_bindings.extend(
                generate_external_variable_binding(0, existing_binding_count, entry.value())
                    .chars(),
            );
            existing_binding_count += 1;
        }

        let mut texture_usage = GraphTextureUsageRecorder::default();
        texture_usage.use_texture(TextureId::NULL);

        let input_sampling = compile_input_sampling()?;

        let main_graph = compile_template_main(
            &self.main_graph,
            &mut texture_usage,
            &external_variable_bindings,
        )?;

        let stroke_postprocess_graphs = compile_template_stroke_postprocess(
            &self.stroke_postprocess_graphs,
            &mut texture_usage,
            &external_variable_bindings,
        )?;

        Ok(CompiledBrushPreset {
            input_sampling,
            main_graph,
            stroke_postprocess_graphs,
            n_stroke_postprocess_graphs: self.stroke_postprocess_graphs.len() as u32,
            texture_usage: texture_usage.used_textures_ordered(),
            external_vars: self.graph_resources.external_vars.clone(),
        })
    }

    pub fn new_stroke_postprocess_graph(&mut self) -> usize {
        let graph = Graph::new(
            self.graph_resources.clone(),
            STROKE_POSTPROCESS_GRAPH_TYPES.clone(),
        );
        self.stroke_postprocess_graphs.push(graph);
        self.stroke_postprocess_graphs.len() - 1
    }

    pub fn main_graph(&self) -> &Graph {
        &self.main_graph
    }

    pub fn main_graph_mut(&mut self) -> &mut Graph {
        self.is_dirty = true;
        &mut self.main_graph
    }

    pub fn stroke_postprocess_graphs(&self) -> &Vec<Graph> {
        &self.stroke_postprocess_graphs
    }

    pub fn stroke_postprocess_graphs_mut(&mut self) -> &mut Vec<Graph> {
        self.is_dirty = true;
        &mut self.stroke_postprocess_graphs
    }

    pub fn metadata(&self) -> &BrushPresetMetadata {
        &self.metadata
    }

    pub fn metadata_mut(&mut self) -> &mut BrushPresetMetadata {
        self.is_dirty = true;
        &mut self.metadata
    }

    pub fn iter_external_vars(
        &self,
    ) -> impl Iterator<Item = (ExternalVariableId, ExternalVariable)> + '_ {
        self.graph_resources
            .external_vars
            .all()
            .iter()
            .map(|entry| {
                let var = entry.value().clone();
                (var.id, var)
            })
    }

    pub fn insert_external_var(&mut self, var: ExternalVariable) {
        self.is_dirty = true;
        self.graph_resources.external_vars.insert(var);
    }

    pub fn update_external_var(
        &self,
        id: &ExternalVariableId,
        msg: ErasedGraphLiteralUpdateMessage,
    ) {
        self.graph_resources.external_vars.update(&id, msg);
    }

    pub fn textures(&self) -> &Arc<GraphTextureStorage> {
        &self.graph_resources.textures
    }

    pub fn functions(&self) -> &Arc<GraphFunctionStorage> {
        &self.graph_resources.functions
    }

    pub fn mark_undirty(&mut self) {
        self.is_dirty = false;
    }

    pub fn is_dirty(&self) -> bool {
        self.is_dirty
    }
}

fn add_modules(resolver: &mut VirtualResolver) {
    resolver.add_module(
        "package::image::texture_unpack".parse().unwrap(),
        include_str!("../../cyancia_image/src/shaders/texture_unpack.wesl").into(),
    );
    resolver.add_module(
        "package::brush::brush_types".parse().unwrap(),
        include_str!("render/brush_types.wesl").into(),
    );
    resolver.add_module(
        "package::render::math".parse().unwrap(),
        include_str!("../../cyancia_render/src/shaders/math.wesl").into(),
    );
    resolver.add_module(
        "package::render::hash".parse().unwrap(),
        include_str!("../../cyancia_render/src/shaders/hash.wesl").into(),
    );
    resolver.add_module(
        "package::image::blend_modes".parse().unwrap(),
        include_str!("../../cyancia_image/src/shaders/blend_modes.wesl").into(),
    );
    resolver.add_module(
        "package::image::image_tiling".parse().unwrap(),
        include_str!("../../cyancia_image/src/shaders/image_tiling.wesl").into(),
    );
}

// TODO
// fn compile_input_sampling(factor: &Graph, required: &Graph) -> anyhow::Result<String> {
fn compile_input_sampling() -> anyhow::Result<String> {
    // let (_, factor) = factor.compile(
    //     Vec::new(),
    //     Default::default(),
    //     &mut GraphTextureUsageRecorder::default(),
    // )?;
    // let (_, required) = required.compile(
    //     Vec::new(),
    //     Default::default(),
    //     &mut GraphTextureUsageRecorder::default(),
    // )?;

    let shader = include_str!("render/brush_sample.wesl").to_string();
    // .replace("//CODEGENFLAG_COMPUTED_GRAPH_SPACING_FACTOR", &factor)
    // .replace("//CODEGENFLAG_COMPUTED_GRAPH_REQUIRED_SPACING", &required);

    let mut resolver = VirtualResolver::new();
    resolver.add_module("package::template".parse().unwrap(), shader.into());
    add_modules(&mut resolver);

    let mut compiler = Wesl::new_barebones().set_custom_resolver(resolver);
    compiler.set_mangler(Default::default());
    compiler.set_options(Default::default());
    let shader = compiler
        .compile(&"package::template".parse().unwrap())?
        .to_string();

    Ok(shader)
}

fn compile_template(
    shader: &str,
    external_variable_bindings: &str,
    size_estimation: bool,
    postprocess: bool,
) -> anyhow::Result<String> {
    let shader = include_str!("render/brush_template.wesl")
        .replace("//CODEGENFLAG_COMPILED_GRAPH", &shader)
        .replace(
            "//CODEGENFLAG_EXTERNAL_VARIABLE_BINDINGS",
            external_variable_bindings,
        );

    let mut resolver = VirtualResolver::new();
    resolver.add_module("package::template".parse().unwrap(), shader.into());
    add_modules(&mut resolver);

    let mut compiler = Wesl::new_barebones().set_custom_resolver(resolver);
    compiler.set_mangler(Default::default());
    compiler.set_options(Default::default());
    compiler.set_feature("SIZE_ESTIMATION", size_estimation);
    compiler.set_feature("POSTPROCESS", postprocess);
    let compiled_shader = compiler
        .compile(&"package::template".parse().unwrap())?
        .to_string();

    Ok(compiled_shader)
}

fn compile_template_main(
    graph: &Graph,
    texture_usage: &mut GraphTextureUsageRecorder,
    external_variable_bindings: &str,
) -> anyhow::Result<CompiledBrushGraph> {
    let (_, shader) = graph.compile(Vec::new(), Default::default(), texture_usage)?;

    Ok(CompiledBrushGraph {
        main: compile_template(&shader, external_variable_bindings, false, false)?,
        size_estimation: compile_template(&shader, external_variable_bindings, true, false)?,
    })
}

fn compile_template_stroke_postprocess(
    graphs: &[Graph],
    texture_usage: &mut GraphTextureUsageRecorder,
    external_variable_bindings: &str,
) -> anyhow::Result<CompiledBrushGraph> {
    let compiled_graphs = graphs
        .iter()
        .map(|graph| graph.compile(Default::default(), Default::default(), texture_usage))
        .collect::<Result<Vec<_>, _>>()?;

    let mut concated_graphs_size_estimation = String::new();
    let mut concated_graphs_main = String::new();
    let len = compiled_graphs.len();
    for (i, (_, g)) in compiled_graphs.into_iter().enumerate() {
        concated_graphs_size_estimation.extend(g.chars().chain(['\n']));
        concated_graphs_main.extend(
            format!(
                "
                    wait_for_sample({i});
                    {g}
                    finish_sample_thread();
                    storageBarrier();
                "
            )
            .chars(),
        );
    }

    Ok(CompiledBrushGraph {
        main: compile_template(
            &concated_graphs_main,
            external_variable_bindings,
            false,
            true,
        )?,
        size_estimation: compile_template(
            &concated_graphs_size_estimation,
            external_variable_bindings,
            true,
            true,
        )?,
    })
}

pub const MAIN_GRAPH_TYPES: LazyLock<Arc<GraphTypeRegistry>> = LazyLock::new(main_graph_types);
pub const MAIN_GRAPH_NODES: LazyLock<Arc<GraphNodeRegistry>> = LazyLock::new(main_graph_nodes);
pub const STROKE_POSTPROCESS_GRAPH_TYPES: LazyLock<Arc<GraphTypeRegistry>> =
    LazyLock::new(stroke_postprocess_graph_types);
pub const STROKE_POSTPROCESS_GRAPH_NODES: LazyLock<Arc<GraphNodeRegistry>> =
    LazyLock::new(stroke_postprocess_graph_nodes);

fn main_graph_types() -> Arc<GraphTypeRegistry> {
    let mut types = GraphTypeRegistry::default();
    types.merge(builtin_types());

    types.into()
}

fn main_graph_nodes() -> Arc<GraphNodeRegistry> {
    let mut nodes = GraphNodeRegistry::default();
    nodes.merge(builtin_nodes());

    nodes.register::<PenPositionNode>();
    nodes.register::<DrawDirectionNode>();
    nodes.register::<PixelPositionNode>();
    nodes.register::<FilterWithinMaskNode>();
    nodes.register::<FilterWithinBoundsNode>();
    nodes.register::<OutputColorNode>();
    nodes.register::<PasteTextureNode>();
    nodes.register::<BlendColorNode>();
    nodes.register::<LayerPixelColorNode>();
    nodes.register::<CurrentPixelColorNode>();
    nodes.register::<EllipticalMaskNode>();
    nodes.register::<BlendWithInputNode>();
    nodes.register::<BlendWithLayerNode>();

    nodes.into()
}

fn stroke_postprocess_graph_types() -> Arc<GraphTypeRegistry> {
    let mut types = GraphTypeRegistry::default();
    types.merge(builtin_types());

    types.into()
}

fn stroke_postprocess_graph_nodes() -> Arc<GraphNodeRegistry> {
    let mut nodes = GraphNodeRegistry::default();
    nodes.merge(builtin_nodes());

    nodes.register::<PenPositionNode>();
    nodes.register::<DrawDirectionNode>();
    nodes.register::<PixelPositionNode>();
    nodes.register::<FilterWithinMaskNode>();
    nodes.register::<FilterWithinBoundsNode>();
    nodes.register::<OutputColorNode>();
    nodes.register::<PasteTextureNode>();
    nodes.register::<BlendColorNode>();
    nodes.register::<LayerPixelColorNode>();
    nodes.register::<CurrentPixelColorNode>();
    nodes.register::<StrokeBoundsNode>();
    nodes.register::<EllipticalMaskNode>();
    nodes.register::<BlendWithInputNode>();
    nodes.register::<BlendWithLayerNode>();

    nodes.into()
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
