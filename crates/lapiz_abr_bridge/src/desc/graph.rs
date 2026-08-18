use std::sync::Arc;

use anyhow::Result;
use iced_core::Point;
use lapiz_assets::asset::AssetId;
use lapiz_brush::{
    instance::{BRUSH_GRAPH_TYPES, MAIN_GRAPH_NODES, REQUIRED_SPACING_GRAPH_NODES},
    render::graph::{
        ForegroundColorNode, OutputBoundsNode, OutputColorNode, OutputRequiredSpacingNode,
        PenPositionNode, PixelPositionNode,
    },
};
use lapiz_render::texture::Image;
use lapiz_shader_graph::{
    graph::{
        Graph, GraphData, GraphResources,
        node::{GraphNode, GraphNodeId, GraphNodeRegistry},
        texture::TextureId,
    },
    save::SerializableGraph,
    wgsl_std::{
        nodes::{CustomExpressionNode, CustomExpressionNodeState, TextureNode},
        types::{ColorType, F32Type, RectType, TextureType, Vec2FType},
    },
};

use crate::desc::wgsl;

pub fn add_stateful_node<Data, T>(
    graph: &mut Graph<Data>,
    position: Point,
    node: T,
    state: T::State,
) -> GraphNodeId
where
    Data: GraphData,
    T: GraphNode<Data>,
{
    let node_id = graph.add_node(position, node);
    graph.update_node_state::<T>(node_id, |current| *current = state);
    node_id
}

pub fn computed_graphs(
    diameter: f32,
    hardness: f32,
    angle: f32,
    roundness: f32,
    spacing: f32,
    flip_x: bool,
    flip_y: bool,
) -> Result<(SerializableGraph, SerializableGraph)> {
    let required_spacing_graph = required_spacing_graph(diameter, spacing)?;
    let main_graph = computed_main_graph(diameter, hardness, angle, roundness, flip_x, flip_y)?;
    Ok((required_spacing_graph, main_graph))
}

pub fn sampled_graphs(
    sample_asset: AssetId<Image>,
    diameter: f32,
    angle: f32,
    roundness: f32,
    spacing: f32,
    flip_x: bool,
    flip_y: bool,
) -> Result<(SerializableGraph, SerializableGraph)> {
    let required_spacing_graph = required_spacing_graph(diameter, spacing)?;
    let main_graph = sampled_main_graph(sample_asset, diameter, angle, roundness, flip_x, flip_y)?;
    Ok((required_spacing_graph, main_graph))
}

fn required_spacing_graph(diameter: f32, spacing: f32) -> Result<SerializableGraph> {
    let mut graph = Graph::new(graph_resources(REQUIRED_SPACING_GRAPH_NODES.clone()));
    let mut state = CustomExpressionNodeState::default();
    state.add_output::<F32Type>(wgsl::REQUIRED_SPACING_OUTPUT);
    state.set_code(wgsl::computed_required_spacing(diameter, spacing));

    let expression = add_stateful_node(
        &mut graph,
        Point::new(100.0, 100.0),
        CustomExpressionNode,
        state,
    );
    let output = graph.add_node(Point::new(300.0, 100.0), OutputRequiredSpacingNode);
    graph.connect_slots_by_index(expression, 0, output, 0);

    Ok(graph.as_serialized()?)
}

fn computed_main_graph(
    diameter: f32,
    hardness: f32,
    angle: f32,
    roundness: f32,
    flip_x: bool,
    flip_y: bool,
) -> Result<SerializableGraph> {
    let mut graph = Graph::new(graph_resources(MAIN_GRAPH_NODES.clone()));
    let pixel_position = graph.add_node(Point::new(0.0, 0.0), PixelPositionNode);
    let pen_position = graph.add_node(Point::new(0.0, 100.0), PenPositionNode);
    let foreground_color = graph.add_node(Point::new(0.0, 200.0), ForegroundColorNode);

    let mut state = CustomExpressionNodeState::default();
    state.add_input::<Vec2FType>(wgsl::MAIN_PIXEL_POSITION_INPUT);
    state.add_input::<Vec2FType>(wgsl::MAIN_PEN_POSITION_INPUT);
    state.add_input::<ColorType>(wgsl::MAIN_FOREGROUND_COLOR_INPUT);
    state.add_output::<ColorType>(wgsl::MAIN_COLOR_OUTPUT);
    state.add_output::<RectType>(wgsl::MAIN_BOUNDS_OUTPUT);
    state.set_code(wgsl::computed_main(
        diameter, hardness, angle, roundness, flip_x, flip_y,
    ));

    let expression = add_stateful_node(
        &mut graph,
        Point::new(300.0, 100.0),
        CustomExpressionNode,
        state,
    );
    let output_color = graph.add_node(Point::new(550.0, 50.0), OutputColorNode);
    let output_bounds = graph.add_node(Point::new(550.0, 150.0), OutputBoundsNode);

    graph.connect_slots_by_index(pixel_position, 0, expression, 0);
    graph.connect_slots_by_index(pen_position, 0, expression, 1);
    graph.connect_slots_by_index(foreground_color, 0, expression, 2);
    graph.connect_slots_by_index(expression, 0, output_color, 0);
    graph.connect_slots_by_index(expression, 1, output_bounds, 0);

    Ok(graph.as_serialized()?)
}

fn sampled_main_graph(
    sample_asset: AssetId<Image>,
    diameter: f32,
    angle: f32,
    roundness: f32,
    flip_x: bool,
    flip_y: bool,
) -> Result<SerializableGraph> {
    let mut graph = Graph::new(graph_resources(MAIN_GRAPH_NODES.clone()));
    let pixel_position = graph.add_node(Point::new(0.0, 0.0), PixelPositionNode);
    let pen_position = graph.add_node(Point::new(0.0, 100.0), PenPositionNode);
    let foreground_color = graph.add_node(Point::new(0.0, 200.0), ForegroundColorNode);
    let texture = add_stateful_node(
        &mut graph,
        Point::new(0.0, 300.0),
        TextureNode,
        TextureId(Some(sample_asset)),
    );

    let mut state = CustomExpressionNodeState::default();
    state.add_input::<Vec2FType>(wgsl::MAIN_PIXEL_POSITION_INPUT);
    state.add_input::<Vec2FType>(wgsl::MAIN_PEN_POSITION_INPUT);
    state.add_input::<ColorType>(wgsl::MAIN_FOREGROUND_COLOR_INPUT);
    state.add_input::<TextureType>(wgsl::MAIN_TIP_TEXTURE_INPUT);
    state.add_output::<ColorType>(wgsl::MAIN_COLOR_OUTPUT);
    state.add_output::<RectType>(wgsl::MAIN_BOUNDS_OUTPUT);
    state.set_code(wgsl::sampled_main(
        diameter, angle, roundness, flip_x, flip_y,
    ));

    let expression = add_stateful_node(
        &mut graph,
        Point::new(300.0, 150.0),
        CustomExpressionNode,
        state,
    );
    let output_color = graph.add_node(Point::new(550.0, 100.0), OutputColorNode);
    let output_bounds = graph.add_node(Point::new(550.0, 200.0), OutputBoundsNode);

    graph.connect_slots_by_index(pixel_position, 0, expression, 0);
    graph.connect_slots_by_index(pen_position, 0, expression, 1);
    graph.connect_slots_by_index(foreground_color, 0, expression, 2);
    graph.connect_slots_by_index(texture, 0, expression, 3);
    graph.connect_slots_by_index(expression, 0, output_color, 0);
    graph.connect_slots_by_index(expression, 1, output_bounds, 0);

    Ok(graph.as_serialized()?)
}

fn graph_resources<Data: GraphData>(
    node_registry: Arc<GraphNodeRegistry<Data>>,
) -> GraphResources<Data> {
    GraphResources {
        type_registry: BRUSH_GRAPH_TYPES.clone(),
        node_registry,
        textures: lapiz_shader_graph::graph::texture::ASSET_GRAPH_TEXTURE_STORAGE.clone(),
        functions: lapiz_shader_graph::graph::function::ASSET_GRAPH_FUNCTION_STORAGE.clone(),
        external_vars: Arc::new(
            lapiz_shader_graph::graph::external::GraphExternalVariableStorage::default(),
        ),
    }
}
