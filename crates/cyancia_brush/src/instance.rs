use std::{
    collections::{HashMap, HashSet},
    fmt::Display,
    io::{Cursor, Read, Write},
    path::Path,
    sync::{
        Arc, LazyLock,
        atomic::{AtomicU64, Ordering},
    },
};

use cyancia_assets::asset::{Asset, AssetHandle, AssetId};
use cyancia_shader_graph::{
    graph::{
        Graph, GraphCompileError, GraphResources,
        external::{
            ExternalVariable, ExternalVariableId, GraphExternalVariableStorage,
            generate_external_variable_binding,
        },
        function::{GraphFunction, GraphFunctionStorage},
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
use serde::{Deserialize, Serialize};
use wesl::{VirtualResolver, Wesl};

use crate::{
    asset::{BrushPreset, BrushPresetMetadata},
    render::graph::{
        BlendColorNode, BlendWithInputNode, BlendWithLayerNode, CurrentPixelColorNode,
        DrawDirectionNode, DrawDirectionsNode, EllipticalMaskNode, FilterWithinBoundsNode,
        FilterWithinMaskNode, LayerPixelColorNode, OutputColorNode, OutputRequiredSpacingNode,
        OutputSpacingNode, PasteTextureNode, PenPositionNode, PenPositionsNode, PixelPositionNode,
        StrokeBoundsNode,
    },
};

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

    spacing_factor_graph: Graph,
    required_spacing_graph: Graph,
    main_graph: Graph,
    stroke_postprocess_graphs: Vec<Graph>,
    graph_resources: GraphResources,
    runtime_revision: AtomicU64,
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
                var.deserialize(BRUSH_GRAPH_TYPES.as_ref())
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

        let required_spacing_graph = {
            let (g, e) = Graph::from_serialized(
                &preset.required_spacing_graph,
                resources.clone(),
                BRUSH_GRAPH_TYPES.clone(),
                REQUIRED_SPACING_GRAPH_NODES.as_ref(),
            );
            errors.extend(e);
            match g {
                Some(g) => g,
                None => return (None, errors),
            }
        };

        let spacing_factor_graph = {
            let (g, e) = Graph::from_serialized(
                &preset.spacing_factor_graph,
                resources.clone(),
                BRUSH_GRAPH_TYPES.clone(),
                SPACING_FACTOR_GRAPH_NODES.as_ref(),
            );
            errors.extend(e);
            match g {
                Some(g) => g,
                None => return (None, errors),
            }
        };

        let main_graph = {
            let (g, e) = Graph::from_serialized(
                &preset.main_graph,
                resources.clone(),
                BRUSH_GRAPH_TYPES.clone(),
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
                BRUSH_GRAPH_TYPES.clone(),
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
                required_spacing_graph,
                spacing_factor_graph,
                main_graph,
                stroke_postprocess_graphs,
                graph_resources: resources,
                runtime_revision: AtomicU64::new(0),
            }),
            errors,
        )
    }

    pub fn as_asset(&self) -> anyhow::Result<BrushPreset> {
        let spacing_factor_graph = self.spacing_factor_graph.as_serialized()?;
        let required_spacing_graph = self.required_spacing_graph.as_serialized()?;
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
            spacing_factor_graph,
            required_spacing_graph,
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

        let input_sampling =
            compile_input_sampling(&self.spacing_factor_graph, &self.required_spacing_graph)?;

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
        let graph = Graph::new(self.graph_resources.clone(), BRUSH_GRAPH_TYPES.clone());
        self.stroke_postprocess_graphs.push(graph);
        self.stroke_postprocess_graphs.len() - 1
    }

    pub fn required_spacing_graph(&self) -> &Graph {
        &self.required_spacing_graph
    }

    pub fn required_spacing_graph_mut(&mut self) -> &mut Graph {
        self.increment_runtime_revision();
        &mut self.required_spacing_graph
    }

    pub fn spacing_factor_graph(&self) -> &Graph {
        &self.spacing_factor_graph
    }

    pub fn spacing_factor_graph_mut(&mut self) -> &mut Graph {
        self.increment_runtime_revision();
        &mut self.spacing_factor_graph
    }

    pub fn main_graph(&self) -> &Graph {
        &self.main_graph
    }

    pub fn main_graph_mut(&mut self) -> &mut Graph {
        self.increment_runtime_revision();
        &mut self.main_graph
    }

    pub fn stroke_postprocess_graphs(&self) -> &Vec<Graph> {
        &self.stroke_postprocess_graphs
    }

    pub fn stroke_postprocess_graphs_mut(&mut self) -> &mut Vec<Graph> {
        self.increment_runtime_revision();
        &mut self.stroke_postprocess_graphs
    }

    pub fn metadata(&self) -> &BrushPresetMetadata {
        &self.metadata
    }

    pub fn metadata_mut(&mut self) -> &mut BrushPresetMetadata {
        self.increment_runtime_revision();
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
        self.increment_runtime_revision();
        self.graph_resources.external_vars.insert(var);
    }

    pub fn update_external_var(
        &self,
        id: &ExternalVariableId,
        msg: ErasedGraphLiteralUpdateMessage,
    ) {
        self.graph_resources.external_vars.update(&id, msg);
    }

    pub fn remove_external_var(&mut self, id: &ExternalVariableId) {
        self.increment_runtime_revision();
        self.graph_resources.external_vars.remove(id);
    }

    pub fn textures(&self) -> &Arc<GraphTextureStorage> {
        &self.graph_resources.textures
    }

    pub fn functions(&self) -> &Arc<GraphFunctionStorage> {
        &self.graph_resources.functions
    }

    pub fn runtime_revision(&self) -> u64 {
        self.runtime_revision.load(Ordering::Acquire)
    }

    fn increment_runtime_revision(&self) {
        self.runtime_revision.fetch_add(1, Ordering::AcqRel);
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

fn compile_input_sampling(factor: &Graph, required: &Graph) -> anyhow::Result<String> {
    let (_, factor) = factor.compile(
        Vec::new(),
        Default::default(),
        &mut GraphTextureUsageRecorder::default(),
    )?;
    let (_, required) = required.compile(
        Vec::new(),
        Default::default(),
        &mut GraphTextureUsageRecorder::default(),
    )?;

    let shader = include_str!("render/brush_sample.wesl")
        .to_string()
        .replace("//CODEGENFLAG_COMPUTED_GRAPH_SPACING_FACTOR", &factor)
        .replace("//CODEGENFLAG_COMPUTED_GRAPH_REQUIRED_SPACING", &required);

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

pub const BRUSH_GRAPH_TYPES: LazyLock<Arc<GraphTypeRegistry>> = LazyLock::new(brush_graph_types);
pub const REQUIRED_SPACING_GRAPH_NODES: LazyLock<Arc<GraphNodeRegistry>> =
    LazyLock::new(required_spacing_graph_nodes);
pub const SPACING_FACTOR_GRAPH_NODES: LazyLock<Arc<GraphNodeRegistry>> =
    LazyLock::new(spacing_factor_graph_nodes);
pub const MAIN_GRAPH_NODES: LazyLock<Arc<GraphNodeRegistry>> = LazyLock::new(main_graph_nodes);
pub const STROKE_POSTPROCESS_GRAPH_NODES: LazyLock<Arc<GraphNodeRegistry>> =
    LazyLock::new(stroke_postprocess_graph_nodes);

fn brush_graph_types() -> Arc<GraphTypeRegistry> {
    let mut types = GraphTypeRegistry::default();
    types.merge(builtin_types());

    types.into()
}

fn required_spacing_graph_nodes() -> Arc<GraphNodeRegistry> {
    let mut nodes = GraphNodeRegistry::default();
    nodes.merge(builtin_nodes());

    nodes.register::<PenPositionNode>();
    nodes.register::<DrawDirectionNode>();
    nodes.register::<OutputRequiredSpacingNode>();

    nodes.into()
}

fn spacing_factor_graph_nodes() -> Arc<GraphNodeRegistry> {
    let mut nodes = GraphNodeRegistry::default();
    nodes.merge(builtin_nodes());

    nodes.register::<PenPositionsNode>();
    nodes.register::<DrawDirectionsNode>();
    nodes.register::<OutputSpacingNode>();

    nodes.into()
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

pub struct GraphFunctionInstance {
    graph_function: GraphFunction,
    runtime_revision: AtomicU64,
}

impl GraphFunctionInstance {
    pub fn new(graph_function: GraphFunction) -> Self {
        Self {
            graph_function,
            runtime_revision: AtomicU64::new(0),
        }
    }

    pub fn graph_function(&self) -> &GraphFunction {
        &self.graph_function
    }

    pub fn graph_function_mut(&mut self) -> &mut GraphFunction {
        self.increment_runtime_revision();
        &mut self.graph_function
    }

    pub fn runtime_revision(&self) -> u64 {
        self.runtime_revision.load(Ordering::Acquire)
    }

    fn increment_runtime_revision(&self) {
        self.runtime_revision.fetch_add(1, Ordering::AcqRel);
    }
}
