use std::fmt::Display;
use std::sync::Arc;

use lapiz_assets::asset::{AssetHandle, AssetId};
use lapiz_runtime::Services;
use lapiz_shader_graph::{
    graph::{
        Graph, GraphResources,
        external::{
            ExternalVariable, ExternalVariableId, GraphExternalVariableStorage,
            generate_external_variable_binding,
        },
        function::{ASSET_GRAPH_FUNCTION_STORAGE, SharedGraphFunctionStorage},
        slot::ErasedGraphLiteralUpdateMessage,
        texture::{
            ASSET_GRAPH_TEXTURE_STORAGE, GraphTextureUsageRecorder, SharedGraphTextureStorage,
        },
    },
    save::{GraphDeserializeError, SerializableExternalVariable},
};
use wesl::{VirtualResolver, Wesl};

use crate::{
    asset::{
        FilterGroupId, FilterPreset, FilterPresetMetadata, FilterSlotRef, SerializableFilterGroup,
    },
    render::graph::FilterGraphData,
};

// Re-export the shared registries from the instance module so the editor and
// panel can reference `crate::instance::FILTER_GRAPH_TYPES` / `FILTER_GRAPH_NODES`.
pub use crate::render::graph::{FILTER_GRAPH_NODES, FILTER_GRAPH_TYPES};

/// Base binding index for external variable storage buffers. Bindings 0..8 are
/// reserved for the fixed filter bindings (see filter_template.wesl).
pub const EXTERNAL_VARIABLE_BASE_BINDING: u32 = 32;

pub struct CompiledFilterGroup {
    pub id: FilterGroupId,
    pub input: FilterSlotRef,
    pub output: FilterSlotRef,
    pub main: String,
    pub bounds_eval: String,
}

pub struct CompiledFilterPreset {
    pub groups: Vec<CompiledFilterGroup>,
    pub texture_usage: GraphTextureUsageRecorder,
    pub external_vars: Arc<GraphExternalVariableStorage>,
}

impl Display for CompiledFilterPreset {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "-------------- Compiled filter preset --------------")?;
        for group in &self.groups {
            writeln!(f, "-------------- Filter group {} --------------", group.id)?;
            writeln!(f, "main:\n{}", group.main)?;
            writeln!(f, "bounds_eval:\n{}", group.bounds_eval)?;
        }
        Ok(())
    }
}

pub struct FilterGroup {
    pub id: FilterGroupId,
    pub name: String,
    pub input: FilterSlotRef,
    pub output: FilterSlotRef,
    pub graph: Graph<FilterGraphData>,
}

pub struct FilterInstance {
    filter_id: Option<AssetId<FilterPreset>>,
    metadata: FilterPresetMetadata,
    textures: SharedGraphTextureStorage,
    functions: SharedGraphFunctionStorage,
    groups: Vec<FilterGroup>,
    external_vars: Arc<GraphExternalVariableStorage>,
}

impl FilterInstance {
    pub fn from_asset(
        handle: &AssetHandle<FilterPreset>,
        _services: &Services,
    ) -> (Option<Self>, Vec<GraphDeserializeError>) {
        let Ok(preset) = handle.get() else {
            log::warn!("Filter preset asset is not loaded yet");
            return (None, Vec::new());
        };
        let (mut instance, errors) = Self::new(
            &preset,
            ASSET_GRAPH_TEXTURE_STORAGE.clone(),
            ASSET_GRAPH_FUNCTION_STORAGE.clone(),
        );
        if let Some(instance) = instance.as_mut() {
            instance.filter_id = Some(handle.id());
        }
        (instance, errors)
    }

    pub fn new(
        preset: &FilterPreset,
        textures: SharedGraphTextureStorage,
        functions: SharedGraphFunctionStorage,
    ) -> (Option<Self>, Vec<GraphDeserializeError>) {
        let external_vars = preset
            .external_vars
            .iter()
            .filter_map(|var| {
                var.deserialize(FILTER_GRAPH_TYPES.as_ref())
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
        let mut groups = Vec::with_capacity(preset.groups.len());
        for serialized in &preset.groups {
            let (g, e) = Graph::<FilterGraphData>::from_serialized(
                &serialized.graph,
                GraphResources {
                    type_registry: FILTER_GRAPH_TYPES.clone(),
                    node_registry: FILTER_GRAPH_NODES.clone(),
                    textures: textures.clone(),
                    functions: functions.clone(),
                    external_vars: external_vars.clone(),
                },
            );
            errors.extend(e);
            let Some(graph) = g else {
                return (None, errors);
            };
            groups.push(FilterGroup {
                id: serialized.id,
                name: serialized.name.clone(),
                input: serialized.input,
                output: serialized.output,
                graph,
            });
        }

        (
            Some(Self {
                filter_id: None,
                metadata: preset.metadata.clone(),
                textures,
                functions,
                groups,
                external_vars,
            }),
            errors,
        )
    }

    pub fn as_asset(&self) -> anyhow::Result<FilterPreset> {
        let groups = self
            .groups
            .iter()
            .map(|group| {
                Ok(SerializableFilterGroup {
                    id: group.id,
                    name: group.name.clone(),
                    input: group.input,
                    output: group.output,
                    graph: group.graph.as_serialized()?,
                })
            })
            .collect::<anyhow::Result<Vec<_>>>()?;
        let external_vars = self
            .external_vars
            .all()
            .iter()
            .map(|entry| SerializableExternalVariable::serialize(entry.value()))
            .collect::<Result<Vec<_>, _>>()?;

        Ok(FilterPreset {
            metadata: self.metadata.clone(),
            groups,
            external_vars,
        })
    }

    /// Kahn topological sort of group execution order based on each group's
    /// `input` reference. A group whose input is `Layer` has no dependency and
    /// is scheduled first; a group whose input is `Group(id)` depends on the
    /// group that produces it, so it runs after.
    pub fn topological_order(&self) -> Vec<usize> {
        use std::collections::{HashMap, VecDeque};

        let n = self.groups.len();
        let index_of = self
            .groups
            .iter()
            .enumerate()
            .map(|(i, g)| (g.id, i))
            .collect::<HashMap<_, _>>();

        let mut consumers: Vec<Vec<usize>> = vec![Vec::new(); n];
        let mut in_degree = vec![0usize; n];

        for (consumer_index, group) in self.groups.iter().enumerate() {
            if let FilterSlotRef::Group(producer_id) = group.input
                && let Some(&producer_index) = index_of.get(&FilterGroupId::new(producer_id))
            {
                consumers[producer_index].push(consumer_index);
                in_degree[consumer_index] += 1;
            }
        }

        let mut queue = (0..n)
            .filter(|&i| in_degree[i] == 0)
            .collect::<VecDeque<_>>();
        let mut order = Vec::with_capacity(n);
        while let Some(i) = queue.pop_front() {
            order.push(i);
            for &consumer in &consumers[i] {
                in_degree[consumer] -= 1;
                if in_degree[consumer] == 0 {
                    queue.push_back(consumer);
                }
            }
        }

        // Defensive: if a cycle somehow remains (it should not after asset
        // validation), append leftover groups so execution is total.
        for i in 0..n {
            if !order.contains(&i) {
                order.push(i);
            }
        }
        order
    }

    /// Index of the final output group (the one whose `output == Layer`).
    pub fn final_group_index(&self) -> usize {
        self.groups
            .iter()
            .position(|g| matches!(g.output, FilterSlotRef::Layer))
            .unwrap_or(0)
    }

    pub fn groups(&self) -> &[FilterGroup] {
        &self.groups
    }

    pub fn groups_mut(&mut self) -> &mut Vec<FilterGroup> {
        &mut self.groups
    }

    pub fn group_count(&self) -> usize {
        self.groups.len()
    }

    /// Appends a new group wired as layer -> layer with a fresh graph, returning
    /// its index. The name defaults to "Group N".
    pub fn new_group(&mut self) -> usize {
        let index = self.groups.len();
        let graph = Graph::new(GraphResources {
            type_registry: FILTER_GRAPH_TYPES.clone(),
            node_registry: FILTER_GRAPH_NODES.clone(),
            textures: self.textures.clone(),
            functions: self.functions.clone(),
            external_vars: self.external_vars.clone(),
        });
        self.groups.push(FilterGroup {
            id: FilterGroupId(uuid::Uuid::new_v4()),
            name: format!("Group {}", index + 1),
            input: FilterSlotRef::Layer,
            output: FilterSlotRef::Layer,
            graph,
        });
        index
    }

    pub fn remove_group(&mut self, index: usize) {
        if index < self.groups.len() {
            self.groups.remove(index);
        }
    }

    /// Move a group from `from` to `to` (both existing indices).
    pub fn move_group(&mut self, from: usize, to: usize) {
        if from >= self.groups.len() || to >= self.groups.len() || from == to {
            return;
        }
        let group = self.groups.remove(from);
        self.groups.insert(to, group);
    }

    pub fn asset_id(&self) -> Option<AssetId<FilterPreset>> {
        self.filter_id
    }

    pub fn metadata(&self) -> &FilterPresetMetadata {
        &self.metadata
    }

    pub fn metadata_mut(&mut self) -> &mut FilterPresetMetadata {
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

    #[tracing::instrument(skip_all, name = "compile_filter_preset")]
    pub fn compile(&self) -> anyhow::Result<CompiledFilterPreset> {
        let mut external_variable_bindings = String::new();
        for (binding, entry) in
            (EXTERNAL_VARIABLE_BASE_BINDING..).zip(self.external_vars.all().iter())
        {
            external_variable_bindings.push_str(&generate_external_variable_binding(
                0,
                binding,
                entry.value(),
            ));
        }

        let mut texture_usage = GraphTextureUsageRecorder::default();

        let mut groups = Vec::with_capacity(self.groups.len());
        for group_index in self.topological_order() {
            let group = &self.groups[group_index];
            let (_, _, shader) =
                group
                    .graph
                    .compile(Vec::new(), Default::default(), &mut texture_usage)?;

            // Detect whether the graph writes its own output bounds so we know
            // whether to fall back to set_output_bounds(input_bounds) in the
            // bounds_eval variant.
            let has_output_bounds = shader.contains("set_output_bounds(");

            let compiled = CompiledFilterGroup {
                id: group.id,
                input: group.input,
                output: group.output,
                main: compile_template(
                    &shader,
                    &external_variable_bindings,
                    false,
                    has_output_bounds,
                )?,
                bounds_eval: compile_template(
                    &shader,
                    &external_variable_bindings,
                    true,
                    has_output_bounds,
                )?,
            };
            groups.push(compiled);
        }

        let external_vars = self.external_vars.clone();

        Ok(CompiledFilterPreset {
            groups,
            texture_usage,
            external_vars,
        })
    }
}

fn add_modules(resolver: &mut VirtualResolver) {
    resolver.add_module(
        "package::image::texture_unpack".parse().unwrap(),
        include_str!("../../lapiz_image/src/shaders/texture_unpack.wesl").into(),
    );
    resolver.add_module(
        "package::render::math".parse().unwrap(),
        include_str!("../../lapiz_render/src/shaders/math.wesl").into(),
    );
    resolver.add_module(
        "package::render::hash".parse().unwrap(),
        include_str!("../../lapiz_render/src/shaders/hash.wesl").into(),
    );
    resolver.add_module(
        "package::image::blend_modes".parse().unwrap(),
        include_str!("../../lapiz_image/src/shaders/blend_modes.wesl").into(),
    );
    resolver.add_module(
        "package::image::image_tiling".parse().unwrap(),
        include_str!("../../lapiz_image/src/shaders/image_tiling.wesl").into(),
    );
}

fn compile_template(
    graph_shader: &str,
    external_variable_bindings: &str,
    bounds_eval: bool,
    has_output_bounds: bool,
) -> anyhow::Result<String> {
    let mut graph_body = graph_shader.to_string();
    if bounds_eval && !has_output_bounds {
        graph_body.push_str("set_output_bounds(input_bounds);\n");
    }

    let shader = include_str!("render/filter_template.wesl")
        .replace("//CODEGENFLAG_COMPILED_GRAPH", &graph_body)
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
    compiler.set_feature("BOUNDS_EVAL", bounds_eval);

    let compiled_shader = compiler
        .compile(&"package::template".parse().unwrap())?
        .to_string();

    Ok(compiled_shader)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use iced_core::Point;
    use lapiz_shader_graph::graph::{
        external::GraphExternalVariableStorage, texture::GraphTextureUsageRecorder,
    };

    use super::compile_template;
    use crate::render::graph::{
        FILTER_GRAPH_NODES, FILTER_GRAPH_TYPES, FilterGraphData, InputColorNode, OutputColorNode,
        PixelPositionNode,
    };

    use lapiz_shader_graph::graph::{Graph, GraphResources};
    use lapiz_shader_graph::wgsl_std::nodes::{GetPixelColorNode, TextureNode};

    #[test]
    fn invert_graph_compiles_both_variants() {
        let mut graph = Graph::<FilterGraphData>::new(GraphResources {
            type_registry: FILTER_GRAPH_TYPES.clone(),
            node_registry: FILTER_GRAPH_NODES.clone(),
            textures: lapiz_shader_graph::graph::texture::ASSET_GRAPH_TEXTURE_STORAGE.clone(),
            functions: lapiz_shader_graph::graph::function::ASSET_GRAPH_FUNCTION_STORAGE.clone(),
            external_vars: Arc::new(GraphExternalVariableStorage::new(vec![])),
        });

        let input = graph.add_node(Point::new(0.0, 0.0), InputColorNode);
        let output = graph.add_node(Point::new(100.0, 0.0), OutputColorNode);
        graph.connect_slots_by_index(input, 0, output, 0);

        let (_, _, shader) = graph
            .compile(
                Vec::new(),
                Default::default(),
                &mut GraphTextureUsageRecorder::default(),
            )
            .expect("graph should compile");

        let main = compile_template(&shader, "", false, false).expect("main variant");
        let bounds = compile_template(&shader, "", true, false).expect("bounds variant");

        assert!(main.contains("fn filter_main"));
        assert!(bounds.contains("fn filter_bounds_eval"));
        assert!(bounds.contains("set_output_bounds(input_bounds)"));
    }

    #[test]
    fn texture_sample_graph_compiles_both_variants() {
        let mut graph = Graph::<FilterGraphData>::new(GraphResources {
            type_registry: FILTER_GRAPH_TYPES.clone(),
            node_registry: FILTER_GRAPH_NODES.clone(),
            textures: lapiz_shader_graph::graph::texture::ASSET_GRAPH_TEXTURE_STORAGE.clone(),
            functions: lapiz_shader_graph::graph::function::ASSET_GRAPH_FUNCTION_STORAGE.clone(),
            external_vars: Arc::new(GraphExternalVariableStorage::new(vec![])),
        });

        let texture = graph.add_node(Point::new(0.0, 0.0), TextureNode);
        let pixel = graph.add_node(Point::new(0.0, 50.0), PixelPositionNode);
        let sample = graph.add_node(Point::new(50.0, 25.0), GetPixelColorNode);
        let output = graph.add_node(Point::new(100.0, 0.0), OutputColorNode);
        graph.connect_slots_by_index(texture, 0, sample, 0);
        graph.connect_slots_by_index(pixel, 0, sample, 1);
        graph.connect_slots_by_index(sample, 0, output, 0);

        let (_, _, shader) = graph
            .compile(
                Vec::new(),
                Default::default(),
                &mut GraphTextureUsageRecorder::default(),
            )
            .expect("graph should compile");

        compile_template(&shader, "", false, false).expect("main variant");
        compile_template(&shader, "", true, false).expect("bounds variant");
    }

    /// End-to-end validation of the builtin .lfp presets: parse each zip,
    /// deserialize into a FilterInstance, and compile both WGSL variants.
    #[test]
    fn builtin_presets_load_and_compile() {
        use lapiz_assets::loader::AssetSerializer;
        use lapiz_shader_graph::graph::function::ASSET_GRAPH_FUNCTION_STORAGE;
        use lapiz_shader_graph::graph::texture::ASSET_GRAPH_TEXTURE_STORAGE;

        use crate::asset::{FilterPreset, FilterPresetSerializer};
        use crate::instance::FilterInstance;

        // Walk up from the crate dir (cargo test runs with cwd = crate dir) to the workspace root.
        let mut dir = std::env::current_dir().expect("cwd");
        let builtin_dir = loop {
            let candidate = dir.join("assets").join("builtin_assets");
            if candidate.is_dir() {
                break candidate;
            }
            if !dir.pop() {
                panic!(
                    "could not locate assets/builtin_assets from {:?}",
                    std::env::current_dir().unwrap()
                );
            }
        };
        let mut presets = Vec::new();
        for entry in std::fs::read_dir(&builtin_dir).expect("read builtin_assets dir") {
            let path = entry.expect("entry").path();
            if path.extension().and_then(|e| e.to_str()) == Some("lfp") {
                let mut file = std::fs::File::open(&path).expect("open lfp");
                let preset: FilterPreset = FilterPresetSerializer
                    .read(&mut file)
                    .unwrap_or_else(|err| panic!("read preset {}: {err}", path.display()));
                presets.push((path, preset));
            }
        }
        assert_eq!(presets.len(), 4, "expected the 4 builtin filter presets");

        for (path, preset) in &presets {
            let (instance, errors) = FilterInstance::new(
                preset,
                ASSET_GRAPH_TEXTURE_STORAGE.clone(),
                ASSET_GRAPH_FUNCTION_STORAGE.clone(),
            );
            assert!(
                errors.is_empty(),
                "{}: graph deserialize errors: {errors:?}",
                path.display()
            );
            let instance = instance.expect("instance construction failed");
            let compiled = instance
                .compile()
                .unwrap_or_else(|err| panic!("{}: compile failed: {err}", path.display()));
            assert!(!compiled.groups.is_empty());
            for group in &compiled.groups {
                assert!(
                    group.main.contains("fn filter_main"),
                    "{}: main variant",
                    path.display()
                );
                assert!(
                    group.bounds_eval.contains("fn filter_bounds_eval"),
                    "{}: bounds variant",
                    path.display()
                );
            }
        }
    }
}
