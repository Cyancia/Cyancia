use std::sync::Arc;

use anyhow::Result;
use iced_core::Point;
use lapiz_assets::asset::AssetId;
use lapiz_brush::{
    instance::{
        BRUSH_GRAPH_TYPES, MAIN_GRAPH_NODES, REQUIRED_SPACING_GRAPH_NODES,
        STROKE_POSTPROCESS_GRAPH_NODES,
    },
    render::graph::{
        BackgroundColorNode, CurrentPixelColorNode, DabIndexNode, DrawDirectionNode,
        ForegroundColorNode, GraphDataWithInitialPenInput, GraphDataWithPenInput,
        InitialDrawDirectionNode, OutputBoundsNode, OutputColorNode, OutputRequiredSpacingNode,
        PenAngleNode, PenPositionNode, PenPressureNode, PenTiltNode, PixelPositionNode,
        StrokeBoundsNode,
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
        nodes::{CustomExpressionNode, CustomExpressionNodeState, TextureNode, TimeNode},
        types::{ColorType, F32Type, I32Type, RectType, TextureType, Vec2FType},
    },
};

use crate::desc::wgsl::{
    AZIMUTH_INPUT, BrushPose, ColorAdjustment, DAB_INDEX_INPUT, DIRECTION_INPUT, Dynamics,
    INITIAL_DIRECTION_INPUT, MAIN_BACKGROUND_COLOR_INPUT, MAIN_BOUNDS_OUTPUT, MAIN_COLOR_OUTPUT,
    MAIN_FOREGROUND_COLOR_INPUT, MAIN_PEN_POSITION_INPUT, MAIN_PIXEL_POSITION_INPUT,
    MAIN_TIP_TEXTURE_INPUT, POSTPROCESS_INPUT_COLOR, POSTPROCESS_STROKE_BOUNDS_INPUT,
    PRESSURE_INPUT, REQUIRED_SPACING_OUTPUT, STROKE_BEGIN_INPUT, Scatter, TILT_INPUT,
    computed_main, computed_required_spacing, opacity_postprocess, sampled_main,
};

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

fn add_dynamics_input_slots(state: &mut CustomExpressionNodeState) {
    state.add_input::<F32Type>(PRESSURE_INPUT);
    state.add_input::<Vec2FType>(TILT_INPUT);
    state.add_input::<F32Type>(AZIMUTH_INPUT);
    state.add_input::<F32Type>(DIRECTION_INPUT);
    state.add_input::<F32Type>(INITIAL_DIRECTION_INPUT);
    state.add_input::<I32Type>(DAB_INDEX_INPUT);
}

fn connect_dynamics_input_nodes<Data>(
    graph: &mut Graph<Data>,
    expression: GraphNodeId,
    input_offset: usize,
) where
    Data: GraphDataWithPenInput + GraphDataWithInitialPenInput,
{
    let pressure = graph.add_node(Point::new(0.0, 350.0), PenPressureNode);
    let tilt = graph.add_node(Point::new(0.0, 400.0), PenTiltNode);
    let angle = graph.add_node(Point::new(0.0, 450.0), PenAngleNode);
    let direction = graph.add_node(Point::new(0.0, 500.0), DrawDirectionNode);
    let initial_direction = graph.add_node(Point::new(0.0, 550.0), InitialDrawDirectionNode);
    let dab_index = graph.add_node(Point::new(0.0, 600.0), DabIndexNode);

    graph.connect_slots_by_index(pressure, 0, expression, input_offset);
    graph.connect_slots_by_index(tilt, 0, expression, input_offset + 1);
    graph.connect_slots_by_index(angle, 1, expression, input_offset + 2);
    graph.connect_slots_by_index(direction, 0, expression, input_offset + 3);
    graph.connect_slots_by_index(initial_direction, 0, expression, input_offset + 4);
    graph.connect_slots_by_index(dab_index, 0, expression, input_offset + 5);
}

pub fn computed_graphs(
    diameter: f32,
    hardness: f32,
    angle: f32,
    roundness: f32,
    spacing: f32,
    flip_x: bool,
    flip_y: bool,
    flow: f32,
    size_dynamics: Option<Dynamics>,
    opacity_dynamics: Option<Dynamics>,
    flow_dynamics: Option<Dynamics>,
    angle_dynamics: Option<Dynamics>,
    roundness_dynamics: Option<Dynamics>,
    tilt_scale: f32,
    pose: BrushPose,
    color_adjustment: Option<ColorAdjustment>,
    scatter: Option<Scatter>,
) -> Result<(SerializableGraph, SerializableGraph)> {
    let required_spacing_graph = required_spacing_graph(diameter, spacing, size_dynamics, pose)?;
    let main_graph = computed_main_graph(
        diameter,
        hardness,
        angle,
        roundness,
        flip_x,
        flip_y,
        flow,
        size_dynamics,
        opacity_dynamics,
        flow_dynamics,
        angle_dynamics,
        roundness_dynamics,
        tilt_scale,
        pose,
        color_adjustment,
        scatter,
    )?;
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
    flow: f32,
    size_dynamics: Option<Dynamics>,
    opacity_dynamics: Option<Dynamics>,
    flow_dynamics: Option<Dynamics>,
    angle_dynamics: Option<Dynamics>,
    roundness_dynamics: Option<Dynamics>,
    tilt_scale: f32,
    pose: BrushPose,
    color_adjustment: Option<ColorAdjustment>,
    scatter: Option<Scatter>,
) -> Result<(SerializableGraph, SerializableGraph)> {
    let required_spacing_graph = required_spacing_graph(diameter, spacing, size_dynamics, pose)?;
    let main_graph = sampled_main_graph(
        sample_asset,
        diameter,
        angle,
        roundness,
        flip_x,
        flip_y,
        flow,
        size_dynamics,
        opacity_dynamics,
        flow_dynamics,
        angle_dynamics,
        roundness_dynamics,
        tilt_scale,
        pose,
        color_adjustment,
        scatter,
    )?;
    Ok((required_spacing_graph, main_graph))
}

fn required_spacing_graph(
    diameter: f32,
    spacing: f32,
    size_dynamics: Option<Dynamics>,
    pose: BrushPose,
) -> Result<SerializableGraph> {
    let mut graph = Graph::new(graph_resources(REQUIRED_SPACING_GRAPH_NODES.clone()));
    let mut state = CustomExpressionNodeState::default();
    add_dynamics_input_slots(&mut state);
    state.add_output::<F32Type>(REQUIRED_SPACING_OUTPUT);
    state.set_code(computed_required_spacing(
        diameter,
        spacing,
        size_dynamics,
        pose,
    ));

    let expression = add_stateful_node(
        &mut graph,
        Point::new(100.0, 100.0),
        CustomExpressionNode,
        state,
    );
    let output = graph.add_node(Point::new(300.0, 100.0), OutputRequiredSpacingNode);
    graph.connect_slots_by_index(expression, 0, output, 0);
    connect_dynamics_input_nodes(&mut graph, expression, 0);

    Ok(graph.as_serialized()?)
}

fn computed_main_graph(
    diameter: f32,
    hardness: f32,
    angle: f32,
    roundness: f32,
    flip_x: bool,
    flip_y: bool,
    flow: f32,
    size_dynamics: Option<Dynamics>,
    opacity_dynamics: Option<Dynamics>,
    flow_dynamics: Option<Dynamics>,
    angle_dynamics: Option<Dynamics>,
    roundness_dynamics: Option<Dynamics>,
    tilt_scale: f32,
    pose: BrushPose,
    color_adjustment: Option<ColorAdjustment>,
    scatter: Option<Scatter>,
) -> Result<SerializableGraph> {
    let mut graph = Graph::new(graph_resources(MAIN_GRAPH_NODES.clone()));
    let pixel_position = graph.add_node(Point::new(0.0, 0.0), PixelPositionNode);
    let pen_position = graph.add_node(Point::new(0.0, 100.0), PenPositionNode);
    let foreground_color = graph.add_node(Point::new(0.0, 200.0), ForegroundColorNode);
    let background_color = graph.add_node(Point::new(0.0, 300.0), BackgroundColorNode);
    let stroke_time = graph.add_node(Point::new(0.0, 650.0), TimeNode);

    let mut state = CustomExpressionNodeState::default();
    state.add_input::<Vec2FType>(MAIN_PIXEL_POSITION_INPUT);
    state.add_input::<Vec2FType>(MAIN_PEN_POSITION_INPUT);
    state.add_input::<ColorType>(MAIN_FOREGROUND_COLOR_INPUT);
    state.add_input::<ColorType>(MAIN_BACKGROUND_COLOR_INPUT);
    add_dynamics_input_slots(&mut state);
    state.add_input::<F32Type>(STROKE_BEGIN_INPUT);
    state.add_output::<ColorType>(MAIN_COLOR_OUTPUT);
    state.add_output::<RectType>(MAIN_BOUNDS_OUTPUT);
    state.set_code(computed_main(
        diameter,
        hardness,
        angle,
        roundness,
        flip_x,
        flip_y,
        flow,
        size_dynamics,
        opacity_dynamics,
        flow_dynamics,
        angle_dynamics,
        roundness_dynamics,
        tilt_scale,
        pose,
        color_adjustment,
        scatter,
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
    graph.connect_slots_by_index(background_color, 0, expression, 3);
    connect_dynamics_input_nodes(&mut graph, expression, 4);
    graph.connect_slots_by_index(stroke_time, 1, expression, 10);
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
    flow: f32,
    size_dynamics: Option<Dynamics>,
    opacity_dynamics: Option<Dynamics>,
    flow_dynamics: Option<Dynamics>,
    angle_dynamics: Option<Dynamics>,
    roundness_dynamics: Option<Dynamics>,
    tilt_scale: f32,
    pose: BrushPose,
    color_adjustment: Option<ColorAdjustment>,
    scatter: Option<Scatter>,
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
    let background_color = graph.add_node(Point::new(0.0, 400.0), BackgroundColorNode);
    let stroke_time = graph.add_node(Point::new(0.0, 650.0), TimeNode);

    let mut state = CustomExpressionNodeState::default();
    state.add_input::<Vec2FType>(MAIN_PIXEL_POSITION_INPUT);
    state.add_input::<Vec2FType>(MAIN_PEN_POSITION_INPUT);
    state.add_input::<ColorType>(MAIN_FOREGROUND_COLOR_INPUT);
    state.add_input::<TextureType>(MAIN_TIP_TEXTURE_INPUT);
    state.add_input::<ColorType>(MAIN_BACKGROUND_COLOR_INPUT);
    add_dynamics_input_slots(&mut state);
    state.add_input::<F32Type>(STROKE_BEGIN_INPUT);
    state.add_output::<ColorType>(MAIN_COLOR_OUTPUT);
    state.add_output::<RectType>(MAIN_BOUNDS_OUTPUT);
    state.set_code(sampled_main(
        diameter,
        angle,
        roundness,
        flip_x,
        flip_y,
        flow,
        size_dynamics,
        opacity_dynamics,
        flow_dynamics,
        angle_dynamics,
        roundness_dynamics,
        tilt_scale,
        pose,
        color_adjustment,
        scatter,
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
    graph.connect_slots_by_index(background_color, 0, expression, 4);
    connect_dynamics_input_nodes(&mut graph, expression, 5);
    graph.connect_slots_by_index(stroke_time, 1, expression, 11);
    graph.connect_slots_by_index(expression, 0, output_color, 0);
    graph.connect_slots_by_index(expression, 1, output_bounds, 0);

    Ok(graph.as_serialized()?)
}

pub fn opacity_postprocess_graph(opacity: f32) -> Result<Option<SerializableGraph>> {
    if opacity >= 1.0 {
        return Ok(None);
    }

    let mut graph = Graph::new(graph_resources(STROKE_POSTPROCESS_GRAPH_NODES.clone()));
    let pixel_position = graph.add_node(Point::new(0.0, 0.0), PixelPositionNode);
    let current_color = graph.add_node(Point::new(200.0, 0.0), CurrentPixelColorNode);
    let stroke_bounds = graph.add_node(Point::new(200.0, 150.0), StrokeBoundsNode);

    let mut state = CustomExpressionNodeState::default();
    state.add_input::<ColorType>(POSTPROCESS_INPUT_COLOR);
    state.add_input::<RectType>(POSTPROCESS_STROKE_BOUNDS_INPUT);
    state.add_output::<ColorType>(MAIN_COLOR_OUTPUT);
    state.add_output::<RectType>(MAIN_BOUNDS_OUTPUT);
    state.set_code(opacity_postprocess(opacity));

    let expression = add_stateful_node(
        &mut graph,
        Point::new(400.0, 75.0),
        CustomExpressionNode,
        state,
    );
    let output_color = graph.add_node(Point::new(650.0, 25.0), OutputColorNode);
    let output_bounds = graph.add_node(Point::new(650.0, 125.0), OutputBoundsNode);

    graph.connect_slots_by_index(pixel_position, 0, current_color, 0);
    graph.connect_slots_by_index(current_color, 0, expression, 0);
    graph.connect_slots_by_index(stroke_bounds, 0, expression, 1);
    graph.connect_slots_by_index(expression, 0, output_color, 0);
    graph.connect_slots_by_index(expression, 1, output_bounds, 0);

    Ok(Some(graph.as_serialized()?))
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
