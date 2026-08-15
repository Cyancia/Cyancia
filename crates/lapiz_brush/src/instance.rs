use std::{
    fmt::Display,
    sync::{Arc, LazyLock},
};

use lapiz_assets::asset::{AssetHandle, AssetId};
use lapiz_render::wesl_jit;
use lapiz_shader_graph::{
    graph::{
        Graph, GraphResources,
        external::{
            ExternalVariable, ExternalVariableId, GraphExternalVariableStorage,
            generate_external_variable_binding,
        },
        function::{GraphFunction, SharedGraphFunctionStorage},
        node::GraphNodeRegistry,
        slot::ErasedGraphLiteralUpdateMessage,
        texture::{GraphTextureUsageRecorder, SharedGraphTextureStorage, TextureId},
        variable::GraphTypeRegistry,
    },
    save::{GraphDeserializeError, SerializableExternalVariable},
    wgsl_std::{builtin_nodes, builtin_types, nodes::TimeNode},
};

use crate::{
    asset::{BrushPreset, BrushPresetMetadata},
    render::graph::{
        BackgroundColorNode, BlendColorNode, BlendWithInputNode, BlendWithLayerNode,
        BrushMainGraphData, BrushRequiredSpacingGraphData, BrushStrokePostprocessGraphData,
        CurrentPixelColorNode, DrawDirectionNode, EllipticalMaskNode, FilterWithinBoundsNode,
        FilterWithinMaskNode, ForegroundColorNode, LayerPixelColorNode, OutputBoundsNode,
        OutputColorNode, OutputRequiredSpacingNode, PasteTextureNode, PenAngleNode,
        PenPositionNode, PenPressureNode, PenTiltNode, PixelPositionNode, SelectionMaskNode,
        StrokeBoundsNode,
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

    textures: SharedGraphTextureStorage,
    functions: SharedGraphFunctionStorage,
    required_spacing_graph: Graph<BrushRequiredSpacingGraphData>,
    main_graph: Graph<BrushMainGraphData>,
    stroke_postprocess_graphs: Vec<Graph<BrushStrokePostprocessGraphData>>,
    external_vars: Arc<GraphExternalVariableStorage>,
}

impl BrushPresetInstance {
    pub fn new(
        preset: &BrushPreset,
        textures: SharedGraphTextureStorage,
        functions: SharedGraphFunctionStorage,
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
            let (g, e) = Graph::<BrushRequiredSpacingGraphData>::from_serialized(
                &preset.required_spacing_graph,
                GraphResources {
                    type_registry: BRUSH_GRAPH_TYPES.clone(),
                    node_registry: REQUIRED_SPACING_GRAPH_NODES.clone(),
                    textures: textures.clone(),
                    functions: functions.clone(),
                    external_vars: external_vars.clone(),
                },
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
                    type_registry: BRUSH_GRAPH_TYPES.clone(),
                    node_registry: MAIN_GRAPH_NODES.clone(),
                    textures: textures.clone(),
                    functions: functions.clone(),
                    external_vars: external_vars.clone(),
                },
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
                    type_registry: BRUSH_GRAPH_TYPES.clone(),
                    node_registry: STROKE_POSTPROCESS_GRAPH_NODES.clone(),
                    textures: textures.clone(),
                    functions: functions.clone(),
                    external_vars: external_vars.clone(),
                },
            );
            errors.extend(e);
            match g {
                Some(g) => stroke_postprocess_graphs.push(g),
                None => return (None, errors),
            }
        }

        (
            Some(Self {
                textures,
                functions,
                brush_id: None,
                metadata: preset.metadata.clone(),
                required_spacing_graph,
                main_graph,
                stroke_postprocess_graphs,
                external_vars,
            }),
            errors,
        )
    }

    pub fn from_asset(
        handle: &AssetHandle<BrushPreset>,
        textures: SharedGraphTextureStorage,
        functions: SharedGraphFunctionStorage,
    ) -> (Option<Self>, Vec<GraphDeserializeError>) {
        let preset = handle.get().unwrap();
        let (mut instance, errors) = Self::new(&preset, textures, functions);
        if let Some(instance) = instance.as_mut() {
            instance.brush_id = Some(handle.id());
        }
        (instance, errors)
    }

    pub fn as_asset(&self) -> anyhow::Result<BrushPreset> {
        let required_spacing_graph = self.required_spacing_graph.as_serialized()?;
        let main_graph = self.main_graph.as_serialized()?;
        let stroke_postprocess_graphs = self
            .stroke_postprocess_graphs
            .iter()
            .map(Graph::as_serialized)
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

    #[tracing::instrument(skip_all, name = "compile_brush_preset")]
    pub fn compile(&self, mut existing_binding_count: u32) -> anyhow::Result<CompiledBrushPreset> {
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
            &self.required_spacing_graph,
            &mut texture_usage,
            &external_variable_bindings,
        )?;

        let main_graph = compile_template_main(
            &self.main_graph,
            &mut texture_usage,
            &external_variable_bindings,
        )?;

        let stroke_postprocess_graphs = compile_template_stroke_postprocess(
            self.stroke_postprocess_graphs.iter(),
            &mut texture_usage,
            &external_variable_bindings,
        )?;

        Ok(CompiledBrushPreset {
            input_sampling,
            main_graph,
            stroke_postprocess_graphs,
            texture_usage: texture_usage.used_textures_ordered(),
            external_vars: self.external_vars.clone(),
        })
    }

    pub fn new_stroke_postprocess_graph(&mut self) -> usize {
        let graph = Graph::new(GraphResources {
            type_registry: BRUSH_GRAPH_TYPES.clone(),
            node_registry: STROKE_POSTPROCESS_GRAPH_NODES.clone(),
            textures: self.textures.clone(),
            functions: self.functions.clone(),
            external_vars: self.external_vars.clone(),
        });
        self.stroke_postprocess_graphs.push(graph);
        self.stroke_postprocess_graphs.len() - 1
    }

    pub fn remove_stroke_postprocess_graph(&mut self, index: usize) {
        self.stroke_postprocess_graphs.remove(index);
    }

    pub fn required_spacing_graph(&self) -> &Graph<BrushRequiredSpacingGraphData> {
        &self.required_spacing_graph
    }

    pub fn required_spacing_graph_mut(&mut self) -> &mut Graph<BrushRequiredSpacingGraphData> {
        &mut self.required_spacing_graph
    }

    pub fn main_graph(&self) -> &Graph<BrushMainGraphData> {
        &self.main_graph
    }

    pub fn main_graph_mut(&mut self) -> &mut Graph<BrushMainGraphData> {
        &mut self.main_graph
    }

    pub fn stroke_postprocess_graphs(&self) -> &[Graph<BrushStrokePostprocessGraphData>] {
        &self.stroke_postprocess_graphs
    }

    pub fn stroke_postprocess_graphs_mut(
        &mut self,
    ) -> &mut Vec<Graph<BrushStrokePostprocessGraphData>> {
        &mut self.stroke_postprocess_graphs
    }

    pub fn stroke_postprocess_graph(
        &self,
        index: usize,
    ) -> Option<&Graph<BrushStrokePostprocessGraphData>> {
        self.stroke_postprocess_graphs.get(index)
    }

    pub fn asset_id(&self) -> Option<AssetId<BrushPreset>> {
        self.brush_id
    }

    pub fn metadata(&self) -> &BrushPresetMetadata {
        &self.metadata
    }

    pub fn metadata_mut(&mut self) -> &mut BrushPresetMetadata {
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
        self.external_vars.insert(var);
    }

    pub fn rename_external_var(&mut self, id: &ExternalVariableId, new_name: String) {
        self.external_vars.rename(id, new_name);
    }

    pub fn update_external_var(
        &self,
        id: &ExternalVariableId,
        message: ErasedGraphLiteralUpdateMessage,
    ) {
        self.external_vars.update(id, message);
    }

    pub fn remove_external_var(&mut self, id: &ExternalVariableId) {
        self.external_vars.remove(id);
    }
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

    let compiled_shader = wesl_jit::compile_wesl_with_config_and_include(
        shader,
        &[&lapiz_image::image::PACKAGE, &lapiz_render::render::PACKAGE],
        |resolver| {
            resolver.add_module(
                "package::brush_types".parse().unwrap(),
                include_str!("render/brush_types.wesl").into(),
            );
        },
        |compiler| {
            compiler.set_feature("POSTPROCESS", postprocess);
            compiler.set_feature("BOUNDS_EVAL", bounds_eval);
        },
    )?;

    Ok(compiled_shader)
}

// TODO Graph validation. Some functions are not allowed to use during estimation stage. For example
//      It is not allowed to use a pixel color sampled from previous input to determine the bounds.

fn compile_template_input_sampling(
    graph: &Graph<BrushRequiredSpacingGraphData>,
    texture_usage: &mut GraphTextureUsageRecorder,
    external_variable_bindings: &str,
) -> anyhow::Result<String> {
    let (_, _, shader) = graph.compile(Vec::new(), Default::default(), texture_usage)?;

    let shader = include_str!("render/brush_sample.wesl")
        .replace("//CODEGENFLAG_COMPUTED_GRAPH_REQUIRED_SPACING", &shader)
        .replace(
            "//CODEGENFLAG_EXTERNAL_VARIABLE_BINDINGS",
            external_variable_bindings,
        );

    let compiled_shader = wesl_jit::compile_wesl_with_config_and_include(
        shader,
        &[&lapiz_image::image::PACKAGE, &lapiz_render::render::PACKAGE],
        |resolver| {
            resolver.add_module(
                "package::brush_types".parse().unwrap(),
                include_str!("render/brush_types.wesl").into(),
            );
        },
        |_| {},
    )?;

    Ok(compiled_shader)
}

fn compile_template_main(
    graph: &Graph<BrushMainGraphData>,
    texture_usage: &mut GraphTextureUsageRecorder,
    external_variable_bindings: &str,
) -> anyhow::Result<CompiledGraph> {
    let (_, _, shader) = graph.compile(Vec::new(), Default::default(), texture_usage)?;

    Ok(CompiledGraph {
        main: compile_template(&shader, external_variable_bindings, false, false)?,
        bounds_eval: compile_template(&shader, external_variable_bindings, false, true)?,
    })
}

fn compile_template_stroke_postprocess<'a>(
    graphs: impl Iterator<Item = &'a Graph<BrushStrokePostprocessGraphData>>,
    texture_usage: &mut GraphTextureUsageRecorder,
    external_variable_bindings: &str,
) -> anyhow::Result<Vec<CompiledGraph>> {
    let compiled_graphs = graphs
        .into_iter()
        .map(|graph| graph.compile(Default::default(), Default::default(), texture_usage))
        .collect::<Result<Vec<_>, _>>()?;

    let compiled_brshes = compiled_graphs
        .into_iter()
        .map(|(_, _, shader)| {
            Ok(CompiledGraph {
                main: compile_template(&shader, external_variable_bindings, true, false)?,
                bounds_eval: compile_template(&shader, external_variable_bindings, true, true)?,
            })
        })
        .collect::<anyhow::Result<Vec<CompiledGraph>>>()?;
    Ok(compiled_brshes)
}

pub static BRUSH_GRAPH_TYPES: LazyLock<Arc<GraphTypeRegistry>> =
    LazyLock::new(|| Arc::new(brush_graph_types()));
pub static REQUIRED_SPACING_GRAPH_NODES: LazyLock<
    Arc<GraphNodeRegistry<BrushRequiredSpacingGraphData>>,
> = LazyLock::new(|| Arc::new(required_spacing_graph_nodes()));
pub static MAIN_GRAPH_NODES: LazyLock<Arc<GraphNodeRegistry<BrushMainGraphData>>> =
    LazyLock::new(|| Arc::new(main_graph_nodes()));
pub static STROKE_POSTPROCESS_GRAPH_NODES: LazyLock<
    Arc<GraphNodeRegistry<BrushStrokePostprocessGraphData>>,
> = LazyLock::new(|| Arc::new(stroke_postprocess_graph_nodes()));

fn brush_graph_types() -> GraphTypeRegistry {
    let mut types = GraphTypeRegistry::default();
    types.merge(builtin_types());

    types
}

fn required_spacing_graph_nodes() -> GraphNodeRegistry<BrushRequiredSpacingGraphData> {
    let mut nodes = GraphNodeRegistry::with_capacity();
    nodes.merge(builtin_nodes());

    nodes.register::<PenPositionNode>();
    nodes.register::<PenPressureNode>();
    nodes.register::<PenAngleNode>();
    nodes.register::<PenTiltNode>();
    nodes.register::<DrawDirectionNode>();
    nodes.register::<TimeNode>();
    nodes.register::<OutputRequiredSpacingNode>();
    nodes.register::<ForegroundColorNode>();
    nodes.register::<BackgroundColorNode>();

    nodes
}

fn main_graph_nodes() -> GraphNodeRegistry<BrushMainGraphData> {
    let mut nodes = GraphNodeRegistry::with_capacity();
    nodes.merge(builtin_nodes());

    nodes.register::<PenPositionNode>();
    nodes.register::<PenPressureNode>();
    nodes.register::<PenAngleNode>();
    nodes.register::<PenTiltNode>();
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
    nodes.register::<ForegroundColorNode>();
    nodes.register::<BackgroundColorNode>();

    nodes
}

fn stroke_postprocess_graph_nodes() -> GraphNodeRegistry<BrushStrokePostprocessGraphData> {
    let mut nodes = GraphNodeRegistry::with_capacity();
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
    nodes.register::<ForegroundColorNode>();
    nodes.register::<BackgroundColorNode>();

    nodes
}

pub struct GraphFunctionInstance {
    graph_function: GraphFunction,
}

impl GraphFunctionInstance {
    pub fn new(graph_function: GraphFunction) -> Self {
        Self { graph_function }
    }

    pub fn graph_function(&self) -> &GraphFunction {
        &self.graph_function
    }

    pub fn graph_function_mut(&mut self) -> &mut GraphFunction {
        &mut self.graph_function
    }
}
