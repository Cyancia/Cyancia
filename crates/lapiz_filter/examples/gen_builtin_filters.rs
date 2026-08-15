//! Builtin filter preset generator.
//!
//! Builds the four test filter presets described in plan.md §9 using the
//! shader-graph API (Graph::new -> add_node / connect_slots), assembles each
//! into a `FilterPreset`, and writes the resulting `.lfp` (zip) files into the
//! workspace's `assets/builtin_assets/` directory via `FilterPresetSerializer`.
//!
//! This example is intentionally data-only: it produces assets, it does not
//! render or validate shader compilation. The orchestrator runs it after the
//! other lapiz_filter milestones are integrated.
#![allow(
    clippy::default_constructed_unit_structs,
    clippy::needless_question_mark
)]

use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use anyhow::{Context, Result};
use iced_core::Point;
use uuid::Uuid;

use lapiz_assets::loader::AssetSerializer;
use lapiz_filter::asset::{
    FilterGroupId, FilterPreset, FilterPresetMetadata, FilterPresetSerializer, FilterSlotRef,
    SerializableFilterGroup,
};
use lapiz_filter::render::graph::{
    BlendWithLayerNode, FILTER_GRAPH_NODES, FILTER_GRAPH_TYPES, FilterGraphData, InputBoundsNode,
    InputColorNode, OutputBoundsNode, OutputColorNode, PixelPositionNode, SampleInputColorNode,
    SelectionMaskNode,
};
use lapiz_shader_graph::graph::{
    Graph, GraphResources,
    external::{ExternalVariable, ExternalVariableId, GraphExternalVariableStorage},
    node::GraphNodeId,
    slot::{GraphInputSlotId, GraphOutputSlotId},
    variable::GraphLiteral,
};
use lapiz_shader_graph::save::{SerializableExternalVariable, SerializableGraph};
use lapiz_shader_graph::wgsl_std::nodes::{
    CombineColorComponentsNode, CombineComponentsNode, ExternalVariableNode, ScalarMathNode,
    ScalarMathNodeMode, SplitColorComponentsNode, SplitComponentsNode,
};
use lapiz_shader_graph::wgsl_std::types::F32Type;

// ---------------------------------------------------------------------------
// Small graph-building helpers (mirror `Graph::add_node` / `connect_slots`).
// ---------------------------------------------------------------------------

/// A fresh shader graph wired to the filter type / node registries and sharing
/// the given external-variable storage across all groups / graphs.
fn new_graph(external_vars: &Arc<GraphExternalVariableStorage>) -> Graph<FilterGraphData> {
    Graph::new(GraphResources {
        type_registry: FILTER_GRAPH_TYPES.clone(),
        node_registry: FILTER_GRAPH_NODES.clone(),
        textures: Default::default(),
        functions: Default::default(),
        external_vars: external_vars.clone(),
    })
}

fn input_slot(g: &Graph<FilterGraphData>, node: GraphNodeId, index: usize) -> GraphInputSlotId {
    g.get_node(&node).expect("node exists").inputs[index]
}

fn output_slot(g: &Graph<FilterGraphData>, node: GraphNodeId, index: usize) -> GraphOutputSlotId {
    g.get_node(&node).expect("node exists").outputs[index]
}

fn connect(
    g: &mut Graph<FilterGraphData>,
    from: GraphNodeId,
    from_output: usize,
    to: GraphNodeId,
    to_input: usize,
) {
    let from_slot = output_slot(g, from, from_output);
    let to_slot = input_slot(g, to, to_input);
    g.connect_slots(from_slot, to_slot);
}

/// Set a Float literal on an input slot (e.g. the constant `1.0` in `1 - x`).
fn set_float(g: &mut Graph<FilterGraphData>, node: GraphNodeId, index: usize, value: f32) {
    let slot = input_slot(g, node, index);
    g.set_slot_value(slot, Box::new(value));
}

/// Append `SelectionMask(PixelPosition) -> Opacity` and
/// `BlendWithLayerNode(filtered, opacity)` before the final Output Color node.
/// This is the shader-group replacement for the removed fixed filter blend pass.
fn connect_selection_blend(
    g: &mut Graph<FilterGraphData>,
    pos: f32,
    filtered: GraphNodeId,
    pixel: GraphNodeId,
) -> GraphNodeId {
    let selection = g.add_node(Point::new(pos, pos), SelectionMaskNode::default());
    let blend = g.add_node(
        Point::new(pos + 1.0, pos + 1.0),
        BlendWithLayerNode::default(),
    );
    let out = g.add_node(Point::new(pos + 2.0, pos + 2.0), OutputColorNode::default());

    connect(g, pixel, 0, selection, 0);
    connect(g, filtered, 0, blend, 0);
    connect(g, selection, 0, blend, 1);
    connect(g, blend, 0, out, 0);

    out
}

/// Explicitly declare the output pixel bounds for a builtin graph:
/// `InputBounds -> OutputBounds` (output bounds equal input bounds).
fn connect_output_bounds(g: &mut Graph<FilterGraphData>, pos: f32) {
    let input_bounds = g.add_node(Point::new(pos, pos + 1.0), InputBoundsNode::default());
    let output_bounds = g.add_node(
        Point::new(pos + 1.0, pos + 1.0),
        OutputBoundsNode::default(),
    );

    connect(g, input_bounds, 0, output_bounds, 0);
}

/// Add a Scalar Math node with the requested mode.
fn add_math(g: &mut Graph<FilterGraphData>, pos: Point, mode: ScalarMathNodeMode) -> GraphNodeId {
    let id = g.add_node(pos, ScalarMathNode::default());
    g.update_node_state::<ScalarMathNode>(id, |state| *state = mode);
    id
}

/// Add an External Variable node bound to `var_id` (output float).
fn add_external_var_node(
    g: &mut Graph<FilterGraphData>,
    pos: Point,
    var_id: ExternalVariableId,
) -> GraphNodeId {
    let id = g.add_node(pos, ExternalVariableNode::default());
    g.update_node_state::<ExternalVariableNode>(id, |state| *state = Some(var_id));
    id
}

// ---------------------------------------------------------------------------
// External-variable helpers.
// ---------------------------------------------------------------------------

fn make_external_var(name: &str, value: f32) -> ExternalVariable {
    ExternalVariable {
        id: ExternalVariableId::new(Uuid::new_v4()),
        name: name.to_string(),
        value: GraphLiteral::new::<F32Type>(value),
    }
}

fn storage_for(vars: Vec<ExternalVariable>) -> Arc<GraphExternalVariableStorage> {
    Arc::new(GraphExternalVariableStorage::new(vars))
}

fn serializable_vars(
    storage: &Arc<GraphExternalVariableStorage>,
) -> Result<Vec<SerializableExternalVariable>> {
    storage
        .all()
        .iter()
        .map(|entry| SerializableExternalVariable::serialize(entry.value()))
        .collect::<Result<_, _>>()
        .context("serializing external variables")
}

// ---------------------------------------------------------------------------
// Filter #1: Invert - single group, layer -> layer.
//   InputColor -> Split -> per-channel (1 - c), alpha passthrough -> Combine -> Output
// ---------------------------------------------------------------------------

fn build_invert_graph(
    external_vars: &Arc<GraphExternalVariableStorage>,
) -> Result<SerializableGraph> {
    let mut g = new_graph(external_vars);
    let mut pos = 0.0;

    let pixel = g.add_node(Point::new(pos, pos), PixelPositionNode::default());
    pos += 1.0;
    let input = g.add_node(Point::new(pos, pos), InputColorNode::default());
    pos += 1.0;
    let split = g.add_node(Point::new(pos, pos), SplitColorComponentsNode::default());
    pos += 1.0;
    let sub_r = add_math(&mut g, Point::new(pos, pos), ScalarMathNodeMode::Subtract);
    pos += 1.0;
    let sub_g = add_math(&mut g, Point::new(pos, pos), ScalarMathNodeMode::Subtract);
    pos += 1.0;
    let sub_b = add_math(&mut g, Point::new(pos, pos), ScalarMathNodeMode::Subtract);
    pos += 1.0;
    let combine = g.add_node(Point::new(pos, pos), CombineColorComponentsNode::default());
    pos += 1.0;

    connect(&mut g, input, 0, split, 0);

    // R, G, B -> (1 - c); alpha passes through unchanged (split.A -> combine.A).
    for (channel, sub) in [(0usize, sub_r), (1, sub_g), (2, sub_b)] {
        connect(&mut g, split, channel, sub, 1); // Subtrahend = channel
        set_float(&mut g, sub, 0, 1.0); // Minuend = 1.0
        connect(&mut g, sub, 0, combine, channel);
    }
    connect(&mut g, split, 3, combine, 3);

    // Blend the inverted color over the original layer using the selection mask.
    connect_selection_blend(&mut g, pos, combine, pixel);
    connect_output_bounds(&mut g, pos + 3.0);
    Ok(g.as_serialized()?)
}

// ---------------------------------------------------------------------------
// Filter #2: Brightness - single group, 2 float external vars.
//   c -> ((c - 0.5) * contrast + 0.5) + brightness, alpha passthrough
// ---------------------------------------------------------------------------

fn build_brightness_graph(
    external_vars: &Arc<GraphExternalVariableStorage>,
    brightness_id: ExternalVariableId,
    contrast_id: ExternalVariableId,
) -> Result<SerializableGraph> {
    let mut g = new_graph(external_vars);
    let mut pos = 0.0;

    let pixel = g.add_node(Point::new(pos, pos), PixelPositionNode::default());
    pos += 1.0;
    let input = g.add_node(Point::new(pos, pos), InputColorNode::default());
    pos += 1.0;
    let split = g.add_node(Point::new(pos, pos), SplitColorComponentsNode::default());
    pos += 1.0;
    let contrast = add_external_var_node(&mut g, Point::new(pos, pos), contrast_id);
    pos += 1.0;
    let brightness = add_external_var_node(&mut g, Point::new(pos, pos), brightness_id);
    pos += 1.0;
    let combine = g.add_node(Point::new(pos, pos), CombineColorComponentsNode::default());
    pos += 1.0;

    connect(&mut g, input, 0, split, 0);

    for channel in 0..3 {
        let sub = add_math(&mut g, Point::new(pos, pos), ScalarMathNodeMode::Subtract);
        pos += 1.0;
        let mul = add_math(&mut g, Point::new(pos, pos), ScalarMathNodeMode::Multiply);
        pos += 1.0;
        let add_half = add_math(&mut g, Point::new(pos, pos), ScalarMathNodeMode::Add);
        pos += 1.0;
        let add_bright = add_math(&mut g, Point::new(pos, pos), ScalarMathNodeMode::Add);
        pos += 1.0;

        // c - 0.5 (Minuend = channel value, Subtrahend = 0.5)
        connect(&mut g, split, channel, sub, 0);
        set_float(&mut g, sub, 1, 0.5);
        // (c - 0.5) * contrast
        connect(&mut g, sub, 0, mul, 0);
        connect(&mut g, contrast, 0, mul, 1);
        // + 0.5
        connect(&mut g, mul, 0, add_half, 0);
        set_float(&mut g, add_half, 1, 0.5);
        // + brightness
        connect(&mut g, add_half, 0, add_bright, 0);
        connect(&mut g, brightness, 0, add_bright, 1);

        connect(&mut g, add_bright, 0, combine, channel);
    }

    connect(&mut g, split, 3, combine, 3);

    // Blend the adjusted color over the original layer using the selection mask.
    connect_selection_blend(&mut g, pos, combine, pixel);
    connect_output_bounds(&mut g, pos + 3.0);
    Ok(g.as_serialized()?)
}

// ---------------------------------------------------------------------------
// Filter #3: Pixelate - single group, 1 float external var (Block Size).
//   pos = floor(pixel / block) * block + block * 0.5 -> SampleInputColor -> Output
// ---------------------------------------------------------------------------

fn build_pixelate_graph(
    external_vars: &Arc<GraphExternalVariableStorage>,
    block_size_id: ExternalVariableId,
) -> Result<SerializableGraph> {
    let mut g = new_graph(external_vars);
    let mut pos = 0.0;

    let pixel = g.add_node(Point::new(pos, pos), PixelPositionNode::default());
    pos += 1.0;
    let split = g.add_node(Point::new(pos, pos), SplitComponentsNode::default());
    pos += 1.0;
    let block = add_external_var_node(&mut g, Point::new(pos, pos), block_size_id);
    pos += 1.0;
    let combine = g.add_node(Point::new(pos, pos), CombineComponentsNode::default());
    pos += 1.0;
    let sample = g.add_node(Point::new(pos, pos), SampleInputColorNode::default());
    pos += 1.0;

    connect(&mut g, pixel, 0, split, 0);

    // block * 0.5 (shared offset for both axes)
    let half = add_math(&mut g, Point::new(pos, pos), ScalarMathNodeMode::Multiply);
    pos += 1.0;
    connect(&mut g, block, 0, half, 0);
    set_float(&mut g, half, 1, 0.5);

    // floor(x / block) * block + block * 0.5 (same for y)
    for axis in 0..2 {
        let div = add_math(&mut g, Point::new(pos, pos), ScalarMathNodeMode::Divide);
        pos += 1.0;
        let floor = add_math(&mut g, Point::new(pos, pos), ScalarMathNodeMode::Floor);
        pos += 1.0;
        let scale = add_math(&mut g, Point::new(pos, pos), ScalarMathNodeMode::Multiply);
        pos += 1.0;
        let center = add_math(&mut g, Point::new(pos, pos), ScalarMathNodeMode::Add);
        pos += 1.0;

        // x / block (Dividend = axis, Divisor = block)
        connect(&mut g, split, axis, div, 0);
        connect(&mut g, block, 0, div, 1);
        // floor(...)
        connect(&mut g, div, 0, floor, 0);
        // floor(...) * block
        connect(&mut g, floor, 0, scale, 0);
        connect(&mut g, block, 0, scale, 1);
        // + block * 0.5
        connect(&mut g, scale, 0, center, 0);
        connect(&mut g, half, 0, center, 1);

        connect(&mut g, center, 0, combine, axis);
    }

    connect(&mut g, combine, 0, sample, 0);

    // Blend the pixelated color over the original layer using the selection mask.
    connect_selection_blend(&mut g, pos, sample, pixel);
    connect_output_bounds(&mut g, pos + 3.0);
    Ok(g.as_serialized()?)
}

// ---------------------------------------------------------------------------
// Filter #4: Posterize Invert - two groups.
//   Group "Posterize":  layer -> group2, floor(c * levels) / (levels - 1)
//   Group "Invert":     group1 -> layer, same graph as filter #1
// ---------------------------------------------------------------------------

fn build_posterize_graph(
    external_vars: &Arc<GraphExternalVariableStorage>,
    levels_id: ExternalVariableId,
) -> Result<SerializableGraph> {
    let mut g = new_graph(external_vars);
    let mut pos = 0.0;

    let input = g.add_node(Point::new(pos, pos), InputColorNode::default());
    pos += 1.0;
    let split = g.add_node(Point::new(pos, pos), SplitColorComponentsNode::default());
    pos += 1.0;
    let levels = add_external_var_node(&mut g, Point::new(pos, pos), levels_id);
    pos += 1.0;
    let combine = g.add_node(Point::new(pos, pos), CombineColorComponentsNode::default());
    pos += 1.0;
    let out = g.add_node(Point::new(pos, pos), OutputColorNode::default());

    connect(&mut g, input, 0, split, 0);

    // levels - 1 shared denominator
    let levels_minus_one = add_math(&mut g, Point::new(pos, pos), ScalarMathNodeMode::Subtract);
    pos += 1.0;
    connect(&mut g, levels, 0, levels_minus_one, 0);
    set_float(&mut g, levels_minus_one, 1, 1.0);

    for channel in 0..3 {
        let mul = add_math(&mut g, Point::new(pos, pos), ScalarMathNodeMode::Multiply);
        pos += 1.0;
        let floor = add_math(&mut g, Point::new(pos, pos), ScalarMathNodeMode::Floor);
        pos += 1.0;
        let div = add_math(&mut g, Point::new(pos, pos), ScalarMathNodeMode::Divide);
        pos += 1.0;

        // c * levels
        connect(&mut g, split, channel, mul, 0);
        connect(&mut g, levels, 0, mul, 1);
        // floor(...)
        connect(&mut g, mul, 0, floor, 0);
        // / (levels - 1)
        connect(&mut g, floor, 0, div, 0);
        connect(&mut g, levels_minus_one, 0, div, 1);

        connect(&mut g, div, 0, combine, channel);
    }

    connect(&mut g, split, 3, combine, 3);
    connect(&mut g, combine, 0, out, 0);
    connect_output_bounds(&mut g, pos);
    Ok(g.as_serialized()?)
}

// ---------------------------------------------------------------------------
// Output (file) handling.
// ---------------------------------------------------------------------------

/// Resolve the workspace `assets/builtin_assets` directory by walking up from
/// the current working directory (cargo runs examples with a crate-local cwd,
/// so this is more robust than a bare relative path).
fn builtin_assets_dir() -> PathBuf {
    if let Ok(cwd) = std::env::current_dir() {
        let mut dir = Some(cwd.as_path());
        while let Some(d) = dir {
            let candidate = d.join("assets").join("builtin_assets");
            if candidate.is_dir() {
                return candidate;
            }
            dir = d.parent();
        }
    }
    PathBuf::from("assets").join("builtin_assets")
}

fn write_preset(dir: &Path, name: &str, preset: &FilterPreset) -> Result<PathBuf> {
    std::fs::create_dir_all(dir)
        .with_context(|| format!("creating builtin assets directory {:?}", dir))?;
    let path = dir.join(format!("{name}.lfp"));
    let file = std::fs::File::create(&path).with_context(|| format!("creating {:?}", path))?;
    FilterPresetSerializer
        .write(preset, &mut std::io::BufWriter::new(file))
        .with_context(|| format!("writing builtin filter preset {name:?}"))?;
    Ok(path)
}

// ---------------------------------------------------------------------------
// main
// ---------------------------------------------------------------------------

fn main() -> Result<()> {
    let dir = builtin_assets_dir();
    println!("Builtin filter output directory: {}", dir.display());

    // ---- #1 Invert -----
    {
        let external_vars = storage_for(vec![]);
        let group_id = FilterGroupId::random();
        let graph = build_invert_graph(&external_vars)?;
        let preset = FilterPreset {
            metadata: FilterPresetMetadata {
                name: "Invert".into(),
            },
            groups: vec![SerializableFilterGroup {
                id: group_id,
                name: "Main".into(),
                input: FilterSlotRef::Layer,
                output: FilterSlotRef::Layer,
                graph,
            }],
            external_vars: serializable_vars(&external_vars)?,
        };
        let path = write_preset(&dir, "Invert", &preset)?;
        println!("wrote {}", path.display());
    }

    // ---- #2 Brightness -----
    {
        let brightness = make_external_var("Brightness", 0.0);
        let contrast = make_external_var("Contrast", 1.0);
        let external_vars = storage_for(vec![brightness.clone(), contrast.clone()]);
        let group_id = FilterGroupId::random();
        let graph = build_brightness_graph(&external_vars, brightness.id, contrast.id)?;
        let preset = FilterPreset {
            metadata: FilterPresetMetadata {
                name: "Brightness".into(),
            },
            groups: vec![SerializableFilterGroup {
                id: group_id,
                name: "Main".into(),
                input: FilterSlotRef::Layer,
                output: FilterSlotRef::Layer,
                graph,
            }],
            external_vars: serializable_vars(&external_vars)?,
        };
        let path = write_preset(&dir, "Brightness", &preset)?;
        println!("wrote {}", path.display());
    }

    // ---- #3 Pixelate -----
    {
        let block_size = make_external_var("Block Size", 4.0);
        let external_vars = storage_for(vec![block_size.clone()]);
        let group_id = FilterGroupId::random();
        let graph = build_pixelate_graph(&external_vars, block_size.id)?;
        let preset = FilterPreset {
            metadata: FilterPresetMetadata {
                name: "Pixelate".into(),
            },
            groups: vec![SerializableFilterGroup {
                id: group_id,
                name: "Main".into(),
                input: FilterSlotRef::Layer,
                output: FilterSlotRef::Layer,
                graph,
            }],
            external_vars: serializable_vars(&external_vars)?,
        };
        let path = write_preset(&dir, "Pixelate", &preset)?;
        println!("wrote {}", path.display());
    }

    // ---- #4 Posterize Invert (two groups) -----
    {
        let levels = make_external_var("Levels", 4.0);
        let external_vars = storage_for(vec![levels.clone()]);

        let posterize_id = FilterGroupId::random();
        let invert_id = FilterGroupId::random();

        // Group 1: Posterize, layer -> group2
        let posterize_graph = build_posterize_graph(&external_vars, levels.id)?;
        // Group 2: Invert, group1 -> layer (same construction as filter #1)
        let invert_graph = build_invert_graph(&external_vars)?;

        let preset = FilterPreset {
            metadata: FilterPresetMetadata {
                name: "Posterize Invert".into(),
            },
            groups: vec![
                SerializableFilterGroup {
                    id: posterize_id,
                    name: "Posterize".into(),
                    input: FilterSlotRef::Layer,
                    output: FilterSlotRef::Group(invert_id.0),
                    graph: posterize_graph,
                },
                SerializableFilterGroup {
                    id: invert_id,
                    name: "Invert".into(),
                    input: FilterSlotRef::Group(posterize_id.0),
                    output: FilterSlotRef::Layer,
                    graph: invert_graph,
                },
            ],
            external_vars: serializable_vars(&external_vars)?,
        };
        let path = write_preset(&dir, "Posterize Invert", &preset)?;
        println!("wrote {}", path.display());
    }

    println!("Generated all 4 builtin filter presets.");
    Ok(())
}
