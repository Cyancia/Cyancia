use std::{
    fmt::Display,
    sync::{
        Arc, LazyLock,
        atomic::{AtomicU64, Ordering},
    },
};

use cyancia_assets::asset::{AssetHandle, AssetId};
use cyancia_shader_graph::{
    graph::{
        Graph, GraphResources,
        external::{
            ExternalVariable, ExternalVariableId, GraphExternalVariableStorage,
            generate_external_variable_binding,
        },
        function::{GraphFunction, GraphFunctionStorage},
        node::GraphNodeRegistry,
        texture::{GraphTextureStorage, GraphTextureUsageRecorder, TextureId},
        variable::{GraphLiteralValue, GraphTypeRegistry},
    },
    save::{GraphDeserializeError, SerializableExternalVariable},
    wgsl_std::{builtin_nodes, builtin_types, nodes::TimeNode},
};
use gpui::{App, AppContext, Entity};
use wesl::{VirtualResolver, Wesl};

use crate::{
    asset::{BrushPreset, BrushPresetMetadata},
    render::graph::{
        BlendColorNode, BlendWithInputNode, BlendWithLayerNode, BrushGraphData,
        BrushGraphDataTuple, BrushGraphPostprocessData, CurrentPixelColorNode, DrawDirectionNode,
        DrawDirectionsNode, EllipticalMaskNode, FilterWithinBoundsNode, FilterWithinMaskNode,
        LayerPixelColorNode, OutputBoundsNode, OutputColorNode, OutputRequiredSpacingNode,
        OutputSpacingNode, PasteTextureNode, PenPositionNode, PenPositionsNode, PixelPositionNode,
        SelectionMaskNode, StrokeBoundsNode, TimesNode,
    },
};

pub struct CompiledGraph {
    pub main: String,
    pub bounds_eval: String,
}

pub struct CompiledBrushPreset {
    pub input_sampling: String,
    pub main_graph: CompiledGraph,
    pub stroke_postprocess_graphs: Vec<CompiledGraph>,
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
            self.main_graph.main
        )?;

        writeln!(
            f,
            "-------------- Main graph bounds eval shader -------------- \n{}",
            self.main_graph.bounds_eval
        )?;

        for (i, graph) in self.stroke_postprocess_graphs.iter().enumerate() {
            writeln!(
                f,
                "-------------- Stroke postprocess graph shader {} -------------- \n{}",
                i, graph.main
            )?;

            writeln!(
                f,
                "-------------- Stroke postprocess graph bounds eval shader {} -------------- \n{}",
                i, graph.bounds_eval
            )?;
        }

        writeln!(f, "-------------- Texture usages --------------")?;
        for usage in &self.texture_usage {
            writeln!(f, "  - {}", usage)?;
        }
        Ok(())
    }
}

pub struct BrushPresetInstance {
    brush_id: Option<AssetId<BrushPreset>>,
    metadata: BrushPresetMetadata,

    required_spacing_graph: Entity<Graph<BrushGraphData>>,
    main_graph: Entity<Graph<BrushGraphData>>,
    stroke_postprocess_graphs: Vec<Entity<Graph<BrushGraphPostprocessData>>>,
    textures: Arc<GraphTextureStorage>,
    main_functions: Arc<GraphFunctionStorage<BrushGraphData>>,
    stroke_pp_functions: Arc<GraphFunctionStorage<BrushGraphPostprocessData>>,
    external_vars: Arc<GraphExternalVariableStorage>,
    runtime_revision: AtomicU64,
}

impl BrushPresetInstance {
    pub fn new(
        preset: &BrushPreset,
        textures: Arc<GraphTextureStorage>,
        main_functions: Arc<GraphFunctionStorage<BrushGraphData>>,
        stroke_pp_functions: Arc<GraphFunctionStorage<BrushGraphPostprocessData>>,
        cx: &mut App,
    ) -> (Option<Self>, Vec<GraphDeserializeError>) {
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

        let mut errors = Vec::new();

        let required_spacing_graph = {
            let (g, e) = Graph::from_serialized(
                &preset.required_spacing_graph,
                GraphResources {
                    textures: textures.clone(),
                    functions: main_functions.clone(),
                    external_vars: external_vars.clone(),
                },
                BRUSH_GRAPH_TYPES.clone(),
                REQUIRED_SPACING_GRAPH_NODES.as_ref(),
                cx,
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
                GraphResources {
                    textures: textures.clone(),
                    functions: main_functions.clone(),
                    external_vars: external_vars.clone(),
                },
                BRUSH_GRAPH_TYPES.clone(),
                MAIN_GRAPH_NODES.as_ref(),
                cx,
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
                GraphResources {
                    textures: textures.clone(),
                    functions: stroke_pp_functions.clone(),
                    external_vars: external_vars.clone(),
                },
                BRUSH_GRAPH_TYPES.clone(),
                STROKE_POSTPROCESS_GRAPH_NODES.as_ref(),
                cx,
            );
            errors.extend(e);
            match g {
                Some(g) => stroke_postprocess_graphs.push(g),
                None => return (None, errors),
            }
        }

        let required_spacing_graph = cx.new(|_| required_spacing_graph);
        let main_graph = cx.new(|_| main_graph);
        let stroke_postprocess_graphs = stroke_postprocess_graphs
            .into_iter()
            .map(|g| cx.new(|_| g))
            .collect::<Vec<_>>();

        (
            Some(Self {
                brush_id: None,
                metadata: preset.metadata.clone(),
                required_spacing_graph,
                main_graph,
                stroke_postprocess_graphs,
                textures,
                main_functions,
                stroke_pp_functions,
                external_vars,
                runtime_revision: AtomicU64::new(0),
            }),
            errors,
        )
    }

    pub fn from_asset(
        handle: &AssetHandle<BrushPreset>,
        textures: Arc<GraphTextureStorage>,
        main_functions: Arc<GraphFunctionStorage<BrushGraphData>>,
        stroke_pp_functions: Arc<GraphFunctionStorage<BrushGraphPostprocessData>>,
        cx: &mut App,
    ) -> (Option<Self>, Vec<GraphDeserializeError>) {
        let preset = handle.get().unwrap();
        let (mut instance, errors) =
            Self::new(&preset, textures, main_functions, stroke_pp_functions, cx);
        if let Some(instance) = instance.as_mut() {
            instance.brush_id = Some(handle.id());
        }
        (instance, errors)
    }

    pub fn as_asset(&self, cx: &App) -> anyhow::Result<BrushPreset> {
        let required_spacing_graph = self.required_spacing_graph.read(cx).as_serialized()?;
        let main_graph = self.main_graph.read(cx).as_serialized()?;
        let stroke_postprocess_graphs = self
            .stroke_postprocess_graphs
            .iter()
            .map(|g| g.read(cx).as_serialized())
            .collect::<anyhow::Result<Vec<_>>>()?;
        let external_vars = self
            .external_vars
            .all()
            .iter()
            .map(|entry| SerializableExternalVariable::serialize(entry.value()))
            .collect::<Result<Vec<_>, _>>()?;

        Ok(BrushPreset {
            metadata: self.metadata.clone(),
            required_spacing_graph,
            main_graph,
            stroke_postprocess_graphs,
            external_vars,
        })
    }

    pub fn compile(
        &self,
        mut existing_binding_count: u32,
        cx: &mut App,
    ) -> anyhow::Result<CompiledBrushPreset> {
        let mut external_variable_bindings = String::new();
        for entry in self.external_vars.all().iter() {
            external_variable_bindings.push_str(&generate_external_variable_binding(
                0,
                existing_binding_count,
                entry.value(),
            ));
            existing_binding_count += 1;
        }

        let mut texture_usage = GraphTextureUsageRecorder::default();

        let input_sampling = compile_template_input_sampling(
            self.required_spacing_graph.read(cx),
            &mut texture_usage,
            &external_variable_bindings,
            cx,
        )?;

        let main_graph = compile_template_main(
            self.main_graph.read(cx),
            &mut texture_usage,
            &external_variable_bindings,
            cx,
        )?;

        let stroke_postprocess_graphs = compile_template_stroke_postprocess(
            self.stroke_postprocess_graphs.iter().map(|g| g.read(cx)),
            &mut texture_usage,
            &external_variable_bindings,
            cx,
        )?;

        Ok(CompiledBrushPreset {
            input_sampling,
            main_graph,
            stroke_postprocess_graphs,
            texture_usage: texture_usage.used_textures_ordered(),
            external_vars: self.external_vars.clone(),
        })
    }

    pub fn new_stroke_postprocess_graph(&mut self, cx: &mut App) -> usize {
        let graph = cx.new(|_| {
            Graph::new(
                GraphResources {
                    textures: self.textures.clone(),
                    functions: self.stroke_pp_functions.clone(),
                    external_vars: self.external_vars.clone(),
                },
                BRUSH_GRAPH_TYPES.clone(),
            )
        });
        self.stroke_postprocess_graphs.push(graph);
        self.stroke_postprocess_graphs.len() - 1
    }

    pub fn remove_stroke_postprocess_graph(&mut self, index: usize) {
        self.increment_runtime_revision();
        self.stroke_postprocess_graphs.remove(index);
    }

    pub fn required_spacing_graph(&self) -> &Entity<Graph<BrushGraphData>> {
        &self.required_spacing_graph
    }

    pub fn main_graph(&self) -> &Entity<Graph<BrushGraphData>> {
        &self.main_graph
    }

    pub fn stroke_postprocess_graphs(&self) -> &[Entity<Graph<BrushGraphPostprocessData>>] {
        &self.stroke_postprocess_graphs
    }

    pub fn stroke_postprocess_graph(
        &self,
        index: usize,
    ) -> Option<&Entity<Graph<BrushGraphPostprocessData>>> {
        self.stroke_postprocess_graphs.get(index)
    }

    pub fn asset_id(&self) -> Option<AssetId<BrushPreset>> {
        self.brush_id
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
        self.external_vars.all().iter().map(|entry| {
            let var = entry.value().clone();
            (var.id, var)
        })
    }

    pub fn insert_external_var(&mut self, var: ExternalVariable) {
        self.increment_runtime_revision();
        self.external_vars.insert(var);
    }

    pub fn rename_external_var(&mut self, id: &ExternalVariableId, new_name: String) {
        self.increment_runtime_revision();
        self.external_vars.rename(id, new_name);
    }

    pub fn update_external_var(
        &self,
        id: &ExternalVariableId,
        new_value: Box<dyn GraphLiteralValue>,
    ) {
        self.external_vars.update(id, new_value);
    }

    pub fn remove_external_var(&mut self, id: &ExternalVariableId) {
        self.increment_runtime_revision();
        self.external_vars.remove(id);
    }

    pub fn textures(&self) -> &Arc<GraphTextureStorage> {
        &self.textures
    }

    pub fn main_functions(&self) -> &Arc<GraphFunctionStorage<BrushGraphData>> {
        &self.main_functions
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

fn compile_template(
    shader: &str,
    external_variable_bindings: &str,
    postprocess: bool,
    bounds_eval: bool,
) -> anyhow::Result<String> {
    let shader = include_str!("render/brush_template.wesl")
        .replace("//CODEGENFLAG_COMPILED_GRAPH", shader)
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
    compiler.set_feature("POSTPROCESS", postprocess);
    compiler.set_feature("BOUNDS_EVAL", bounds_eval);
    let compiled_shader = compiler
        .compile(&"package::template".parse().unwrap())?
        .to_string();

    Ok(compiled_shader)
}

// TODO Graph validation. Some functions are not allowed to use during estimation stage. For example
//      It is not allowed to use a pixel color sampled from previous input to determine the bounds.

fn compile_template_input_sampling(
    graph: &Graph<BrushGraphData>,
    texture_usage: &mut GraphTextureUsageRecorder,
    _external_variable_bindings: &str,
    cx: &App,
) -> anyhow::Result<String> {
    // TODO Support external variables
    let (_, shader) = graph.compile(Vec::new(), Default::default(), texture_usage, cx)?;

    let shader = include_str!("render/brush_sample.wesl")
        .replace("//CODEGENFLAG_COMPUTED_GRAPH_REQUIRED_SPACING", &shader);

    let mut resolver = VirtualResolver::new();
    resolver.add_module("package::template".parse().unwrap(), shader.into());
    add_modules(&mut resolver);

    let mut compiler = Wesl::new_barebones().set_custom_resolver(resolver);
    compiler.set_mangler(Default::default());
    compiler.set_options(Default::default());
    let compiled_shader = compiler
        .compile(&"package::template".parse().unwrap())?
        .to_string();

    Ok(compiled_shader)
}

fn compile_template_main(
    graph: &Graph<BrushGraphData>,
    texture_usage: &mut GraphTextureUsageRecorder,
    external_variable_bindings: &str,
    cx: &App,
) -> anyhow::Result<CompiledGraph> {
    let (_, shader) = graph.compile(Vec::new(), Default::default(), texture_usage, cx)?;

    Ok(CompiledGraph {
        main: compile_template(&shader, external_variable_bindings, false, false)?,
        bounds_eval: compile_template(&shader, external_variable_bindings, false, true)?,
    })
}

fn compile_template_stroke_postprocess<'a>(
    graphs: impl Iterator<Item = &'a Graph<BrushGraphPostprocessData>>,
    texture_usage: &mut GraphTextureUsageRecorder,
    external_variable_bindings: &str,
    cx: &App,
) -> anyhow::Result<Vec<CompiledGraph>> {
    let compiled_graphs = graphs
        .into_iter()
        .map(|graph| graph.compile(Default::default(), Default::default(), texture_usage, cx))
        .collect::<Result<Vec<_>, _>>()?;

    let compiled_brshes = compiled_graphs
        .into_iter()
        .map(|(_, shader)| {
            Ok(CompiledGraph {
                main: compile_template(&shader, external_variable_bindings, true, false)?,
                bounds_eval: compile_template(&shader, external_variable_bindings, true, true)?,
            })
        })
        .collect::<anyhow::Result<Vec<CompiledGraph>>>()?;
    Ok(compiled_brshes)
}

pub static BRUSH_GRAPH_TYPES: LazyLock<Arc<GraphTypeRegistry>> = LazyLock::new(brush_graph_types);
pub static REQUIRED_SPACING_GRAPH_NODES: LazyLock<Arc<GraphNodeRegistry<BrushGraphData>>> =
    LazyLock::new(required_spacing_graph_nodes);
pub static SPACING_FACTOR_GRAPH_NODES: LazyLock<Arc<GraphNodeRegistry<BrushGraphDataTuple>>> =
    LazyLock::new(spacing_factor_graph_nodes);
pub static MAIN_GRAPH_NODES: LazyLock<Arc<GraphNodeRegistry<BrushGraphData>>> =
    LazyLock::new(main_graph_nodes);
pub static STROKE_POSTPROCESS_GRAPH_NODES: LazyLock<
    Arc<GraphNodeRegistry<BrushGraphPostprocessData>>,
> = LazyLock::new(stroke_postprocess_graph_nodes);

fn brush_graph_types() -> Arc<GraphTypeRegistry> {
    let mut types = GraphTypeRegistry::default();
    types.merge(builtin_types());

    types.into()
}

fn required_spacing_graph_nodes() -> Arc<GraphNodeRegistry<BrushGraphData>> {
    let mut nodes = GraphNodeRegistry::default();
    nodes.merge(builtin_nodes());

    nodes.register::<PenPositionNode>();
    nodes.register::<DrawDirectionNode>();
    nodes.register::<TimeNode>();
    nodes.register::<OutputRequiredSpacingNode>();

    nodes.into()
}

fn spacing_factor_graph_nodes() -> Arc<GraphNodeRegistry<BrushGraphDataTuple>> {
    let mut nodes = GraphNodeRegistry::default();
    nodes.merge(builtin_nodes());

    nodes.register::<PenPositionsNode>();
    nodes.register::<DrawDirectionsNode>();
    nodes.register::<TimesNode>();
    nodes.register::<OutputSpacingNode>();

    nodes.into()
}

fn main_graph_nodes() -> Arc<GraphNodeRegistry<BrushGraphData>> {
    let mut nodes = GraphNodeRegistry::default();
    nodes.merge(builtin_nodes());

    nodes.register::<PenPositionNode>();
    nodes.register::<DrawDirectionNode>();
    nodes.register::<TimeNode>();
    nodes.register::<PixelPositionNode>();
    nodes.register::<FilterWithinMaskNode>();
    nodes.register::<FilterWithinBoundsNode>();
    nodes.register::<OutputColorNode>();
    nodes.register::<OutputBoundsNode>();
    nodes.register::<PasteTextureNode>();
    nodes.register::<BlendColorNode>();
    nodes.register::<LayerPixelColorNode>();
    nodes.register::<CurrentPixelColorNode>();
    nodes.register::<EllipticalMaskNode>();
    nodes.register::<BlendWithInputNode>();
    nodes.register::<BlendWithLayerNode>();
    nodes.register::<SelectionMaskNode>();

    nodes.into()
}

fn stroke_postprocess_graph_nodes() -> Arc<GraphNodeRegistry<BrushGraphPostprocessData>> {
    let mut nodes = GraphNodeRegistry::default();
    nodes.merge(builtin_nodes());

    nodes.register::<PixelPositionNode>();
    nodes.register::<FilterWithinMaskNode>();
    nodes.register::<FilterWithinBoundsNode>();
    nodes.register::<OutputColorNode>();
    nodes.register::<OutputBoundsNode>();
    nodes.register::<PasteTextureNode>();
    nodes.register::<BlendColorNode>();
    nodes.register::<LayerPixelColorNode>();
    nodes.register::<CurrentPixelColorNode>();
    nodes.register::<StrokeBoundsNode>();
    nodes.register::<EllipticalMaskNode>();
    nodes.register::<BlendWithInputNode>();
    nodes.register::<BlendWithLayerNode>();
    nodes.register::<SelectionMaskNode>();

    nodes.into()
}

pub struct GraphFunctionInstance {
    graph_function: GraphFunction<BrushGraphData>,
    runtime_revision: AtomicU64,
}

impl GraphFunctionInstance {
    pub fn new(graph_function: GraphFunction<BrushGraphData>) -> Self {
        Self {
            graph_function,
            runtime_revision: AtomicU64::new(0),
        }
    }

    pub fn graph_function(&self) -> &GraphFunction<BrushGraphData> {
        &self.graph_function
    }

    pub fn graph_function_mut(&mut self) -> &mut GraphFunction<BrushGraphData> {
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
