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
        StrokeBoundsNode, StrokeDistanceNode,
    },
};
use lapiz_image::blend_modes::BlendMode;
use lapiz_render::texture::Image;
use lapiz_shader_graph::{
    graph::{
        Graph, GraphData, GraphResources,
        external::{ExternalVariable, ExternalVariableId, GraphExternalVariableStorage},
        node::{GraphNode, GraphNodeId, GraphNodeRegistry},
        texture::TextureId,
        variable::GraphLiteral,
    },
    save::SerializableGraph,
    wgsl_std::{
        nodes::{
            CustomExpressionNode, CustomExpressionNodeState, ExternalVariableNode, TextureNode,
            TimeNode,
        },
        types::{ColorType, F32Type, I32Type, RectType, TextureType, Vec2FType},
    },
};
use uuid::Uuid;

use crate::desc::wgsl::{
    AZIMUTH_INPUT, BrushPose, BrushTexture, ColorAdjustment, DAB_INDEX_INPUT, DIRECTION_INPUT,
    DUAL_TIP_TEXTURE_INPUT, DualBrush, Dynamics, INITIAL_DIRECTION_INPUT,
    MAIN_BACKGROUND_COLOR_INPUT, MAIN_BOUNDS_OUTPUT, MAIN_COLOR_OUTPUT,
    MAIN_FOREGROUND_COLOR_INPUT, MAIN_PATTERN_TEXTURE_INPUT, MAIN_PEN_POSITION_INPUT,
    MAIN_PIXEL_POSITION_INPUT, MAIN_TIP_TEXTURE_INPUT, POSTPROCESS_INPUT_COLOR,
    POSTPROCESS_STROKE_BOUNDS_INPUT, PRESSURE_INPUT, REQUIRED_SPACING_OUTPUT, STROKE_BEGIN_INPUT,
    STROKE_DISTANCE_INPUT, Scatter, TILT_INPUT, USER_FLOW, USER_OPACITY, USER_SIZE, computed_main,
    computed_required_spacing, opacity_postprocess, sampled_main, sampled_required_spacing,
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

#[derive(Clone, Copy)]
pub struct MainGraphOptions {
    pub flow: f32,
    pub size_dynamics: Option<Dynamics>,
    pub opacity_dynamics: Option<Dynamics>,
    pub flow_dynamics: Option<Dynamics>,
    pub angle_dynamics: Option<Dynamics>,
    pub roundness_dynamics: Option<Dynamics>,
    pub tilt_scale: f32,
    pub flip_x_jitter: bool,
    pub flip_y_jitter: bool,
    pub pose: BrushPose,
    pub color_adjustment: Option<ColorAdjustment>,
    pub scatter: Option<Scatter>,
    pub brush_texture: Option<BrushTexture>,
    pub pattern_asset: Option<AssetId<Image>>,
    pub noise: bool,
    pub dual_brush: Option<DualBrush>,
    pub dual_sample_asset: Option<AssetId<Image>>,
}

#[derive(Clone, Copy)]
pub struct ComputedMainTip {
    pub diameter: f32,
    pub hardness: f32,
    pub angle: f32,
    pub roundness: f32,
    pub flip_x: bool,
    pub flip_y: bool,
    pub spacing: f32,
}

#[derive(Clone, Copy)]
pub struct SampledMainTip {
    pub sample_asset: AssetId<Image>,
    pub diameter: f32,
    pub angle: f32,
    pub roundness: f32,
    pub flip_x: bool,
    pub flip_y: bool,
    pub spacing: f32,
}

#[derive(Clone, Copy)]
pub enum MainTip {
    Computed(ComputedMainTip),
    Sampled(SampledMainTip),
}

pub fn computed_graphs(
    tip: ComputedMainTip,
    options: MainGraphOptions,
    external_vars: &ExternalVariables,
) -> Result<(SerializableGraph, SerializableGraph)> {
    let required_spacing_graph = required_spacing_graph(
        external_vars,
        tip.spacing,
        options.size_dynamics,
        options.pose,
        None,
    )?;
    let main_graph = build_main_graph(MainTip::Computed(tip), options, external_vars)?;
    Ok((required_spacing_graph, main_graph))
}

pub fn sampled_graphs(
    tip: SampledMainTip,
    options: MainGraphOptions,
    external_vars: &ExternalVariables,
) -> Result<(SerializableGraph, SerializableGraph)> {
    let required_spacing_graph = required_spacing_graph(
        external_vars,
        tip.spacing,
        options.size_dynamics,
        options.pose,
        Some(tip.sample_asset),
    )?;
    let main_graph = build_main_graph(MainTip::Sampled(tip), options, external_vars)?;
    Ok((required_spacing_graph, main_graph))
}

fn required_spacing_graph(
    external_vars: &ExternalVariables,
    spacing: f32,
    size_dynamics: Option<Dynamics>,
    pose: BrushPose,
    sample_asset: Option<AssetId<Image>>,
) -> Result<SerializableGraph> {
    let mut graph = Graph::new(graph_resources(
        REQUIRED_SPACING_GRAPH_NODES.clone(),
        external_vars.storage.clone(),
    ));
    let mut state = CustomExpressionNodeState::default();
    let input_offset = if sample_asset.is_some() {
        state.add_input::<TextureType>(MAIN_TIP_TEXTURE_INPUT);
        1
    } else {
        0
    };
    add_dynamics_input_slots(&mut state);
    state.add_input::<F32Type>(USER_SIZE);
    state.add_output::<F32Type>(REQUIRED_SPACING_OUTPUT);
    state.set_code(if sample_asset.is_some() {
        sampled_required_spacing(spacing, size_dynamics, pose)
    } else {
        computed_required_spacing(spacing, size_dynamics, pose)
    });

    let expression = add_stateful_node(
        &mut graph,
        Point::new(100.0, 100.0),
        CustomExpressionNode,
        state,
    );
    if let Some(sample_asset) = sample_asset {
        let texture = add_stateful_node(
            &mut graph,
            Point::new(0.0, 300.0),
            TextureNode,
            TextureId(Some(sample_asset)),
        );
        graph.connect_slots_by_index(texture, 0, expression, 0);
    }
    let user_size = add_stateful_node(
        &mut graph,
        Point::new(0.0, 650.0),
        ExternalVariableNode,
        Some(external_vars.size),
    );
    let output = graph.add_node(Point::new(300.0, 100.0), OutputRequiredSpacingNode);
    graph.connect_slots_by_index(expression, 0, output, 0);
    connect_dynamics_input_nodes(&mut graph, expression, input_offset);
    graph.connect_slots_by_index(user_size, 0, expression, input_offset + 6);

    graph.as_serialized()
}

fn build_main_graph(
    tip: MainTip,
    options: MainGraphOptions,
    external_vars: &ExternalVariables,
) -> Result<SerializableGraph> {
    let mut graph = Graph::new(graph_resources(
        MAIN_GRAPH_NODES.clone(),
        external_vars.storage.clone(),
    ));
    let pixel_position = graph.add_node(Point::new(0.0, 0.0), PixelPositionNode);
    let pen_position = graph.add_node(Point::new(0.0, 100.0), PenPositionNode);
    let foreground_color = graph.add_node(Point::new(0.0, 200.0), ForegroundColorNode);
    let tip_texture = match &tip {
        MainTip::Sampled(tip) => Some(add_stateful_node(
            &mut graph,
            Point::new(0.0, 300.0),
            TextureNode,
            TextureId(Some(tip.sample_asset)),
        )),
        MainTip::Computed(_) => None,
    };
    let background_color = graph.add_node(Point::new(0.0, 400.0), BackgroundColorNode);
    let pattern_texture = options.pattern_asset.map(|pattern_asset| {
        add_stateful_node(
            &mut graph,
            Point::new(0.0, 450.0),
            TextureNode,
            TextureId(Some(pattern_asset)),
        )
    });
    let stroke_distance = options
        .dual_brush
        .map(|_| graph.add_node(Point::new(0.0, 600.0), StrokeDistanceNode));
    let stroke_time = graph.add_node(Point::new(0.0, 650.0), TimeNode);
    let dual_texture = options.dual_sample_asset.map(|sample_asset| {
        add_stateful_node(
            &mut graph,
            Point::new(0.0, 700.0),
            TextureNode,
            TextureId(Some(sample_asset)),
        )
    });

    let mut state = CustomExpressionNodeState::default();
    state.add_input::<Vec2FType>(MAIN_PIXEL_POSITION_INPUT);
    state.add_input::<Vec2FType>(MAIN_PEN_POSITION_INPUT);
    state.add_input::<ColorType>(MAIN_FOREGROUND_COLOR_INPUT);
    if tip_texture.is_some() {
        state.add_input::<TextureType>(MAIN_TIP_TEXTURE_INPUT);
    }
    state.add_input::<ColorType>(MAIN_BACKGROUND_COLOR_INPUT);
    state.add_input::<TextureType>(MAIN_PATTERN_TEXTURE_INPUT);
    add_dynamics_input_slots(&mut state);
    if stroke_distance.is_some() {
        state.add_input::<F32Type>(STROKE_DISTANCE_INPUT);
    }
    state.add_input::<F32Type>(STROKE_BEGIN_INPUT);
    if dual_texture.is_some() {
        state.add_input::<TextureType>(DUAL_TIP_TEXTURE_INPUT);
    }
    state.add_input::<F32Type>(USER_SIZE);
    state.add_input::<F32Type>(USER_FLOW);
    state.add_output::<ColorType>(MAIN_COLOR_OUTPUT);
    state.add_output::<RectType>(MAIN_BOUNDS_OUTPUT);
    state.set_code(match tip {
        MainTip::Computed(tip) => computed_main(tip, options),
        MainTip::Sampled(tip) => sampled_main(tip, options),
    });

    let expression = add_stateful_node(
        &mut graph,
        Point::new(300.0, 150.0),
        CustomExpressionNode,
        state,
    );
    let user_size = add_stateful_node(
        &mut graph,
        Point::new(100.0, 800.0),
        ExternalVariableNode,
        Some(external_vars.size),
    );
    let user_flow = add_stateful_node(
        &mut graph,
        Point::new(100.0, 850.0),
        ExternalVariableNode,
        Some(external_vars.flow),
    );
    let output_color = graph.add_node(Point::new(550.0, 100.0), OutputColorNode);
    let output_bounds = graph.add_node(Point::new(550.0, 200.0), OutputBoundsNode);
    let tip_input_offset = usize::from(tip_texture.is_some());
    let background_input = 3 + tip_input_offset;
    let pattern_input = background_input + 1;
    let dynamics_input = pattern_input + 1;
    let stroke_distance_input = dynamics_input + 6;
    let stroke_time_input = stroke_distance_input + usize::from(stroke_distance.is_some());
    let user_size_input = stroke_time_input + 1 + usize::from(dual_texture.is_some());

    graph.connect_slots_by_index(pixel_position, 0, expression, 0);
    graph.connect_slots_by_index(pen_position, 0, expression, 1);
    graph.connect_slots_by_index(foreground_color, 0, expression, 2);
    if let Some(tip_texture) = tip_texture {
        graph.connect_slots_by_index(tip_texture, 0, expression, 3);
    }
    graph.connect_slots_by_index(background_color, 0, expression, background_input);
    if let Some(pattern_texture) = pattern_texture {
        graph.connect_slots_by_index(pattern_texture, 0, expression, pattern_input);
    }
    connect_dynamics_input_nodes(&mut graph, expression, dynamics_input);
    if let Some(stroke_distance) = stroke_distance {
        graph.connect_slots_by_index(stroke_distance, 0, expression, stroke_distance_input);
    }
    graph.connect_slots_by_index(stroke_time, 1, expression, stroke_time_input);
    if let Some(dual_texture) = dual_texture {
        graph.connect_slots_by_index(dual_texture, 0, expression, stroke_time_input + 1);
    }
    graph.connect_slots_by_index(user_size, 0, expression, user_size_input);
    graph.connect_slots_by_index(user_flow, 0, expression, user_size_input + 1);

    graph.connect_slots_by_index(expression, 0, output_color, 0);
    graph.connect_slots_by_index(expression, 1, output_bounds, 0);

    graph.as_serialized()
}

pub fn opacity_postprocess_graph(
    opacity: f32,
    blend_mode: BlendMode,
    external_vars: &ExternalVariables,
) -> Result<SerializableGraph> {
    let mut graph = Graph::new(graph_resources(
        STROKE_POSTPROCESS_GRAPH_NODES.clone(),
        external_vars.storage.clone(),
    ));
    let pixel_position = graph.add_node(Point::new(0.0, 0.0), PixelPositionNode);
    let current_color = graph.add_node(Point::new(200.0, 0.0), CurrentPixelColorNode);
    let stroke_bounds = graph.add_node(Point::new(200.0, 150.0), StrokeBoundsNode);

    let mut state = CustomExpressionNodeState::default();
    state.add_input::<ColorType>(POSTPROCESS_INPUT_COLOR);
    state.add_input::<RectType>(POSTPROCESS_STROKE_BOUNDS_INPUT);
    state.add_input::<F32Type>(USER_OPACITY);
    state.add_output::<ColorType>(MAIN_COLOR_OUTPUT);
    state.add_output::<RectType>(MAIN_BOUNDS_OUTPUT);
    state.set_code(opacity_postprocess(opacity, blend_mode));

    let expression = add_stateful_node(
        &mut graph,
        Point::new(400.0, 75.0),
        CustomExpressionNode,
        state,
    );
    let user_opacity = add_stateful_node(
        &mut graph,
        Point::new(200.0, 225.0),
        ExternalVariableNode,
        Some(external_vars.opacity),
    );
    let output_color = graph.add_node(Point::new(650.0, 25.0), OutputColorNode);
    let output_bounds = graph.add_node(Point::new(650.0, 125.0), OutputBoundsNode);

    graph.connect_slots_by_index(pixel_position, 0, current_color, 0);
    graph.connect_slots_by_index(current_color, 0, expression, 0);
    graph.connect_slots_by_index(stroke_bounds, 0, expression, 1);
    graph.connect_slots_by_index(user_opacity, 0, expression, 2);
    graph.connect_slots_by_index(expression, 0, output_color, 0);
    graph.connect_slots_by_index(expression, 1, output_bounds, 0);

    graph.as_serialized()
}

pub struct ExternalVariables {
    pub size: ExternalVariableId,
    pub opacity: ExternalVariableId,
    pub flow: ExternalVariableId,
    pub storage: Arc<GraphExternalVariableStorage>,
}

pub fn external_variables(size: f32) -> ExternalVariables {
    let size_id = ExternalVariableId::new(Uuid::new_v4());
    let opacity_id = ExternalVariableId::new(Uuid::new_v4());
    let flow_id = ExternalVariableId::new(Uuid::new_v4());

    let ext_vars = GraphExternalVariableStorage::default();
    ext_vars.insert(ExternalVariable {
        id: size_id,
        name: "Size".to_string(),
        value: GraphLiteral::new::<F32Type>(size.clamp(0.1, 1000.0)),
    });
    ext_vars.insert(ExternalVariable {
        id: opacity_id,
        name: "Opacity".to_string(),
        value: GraphLiteral::new::<F32Type>(1.0),
    });
    ext_vars.insert(ExternalVariable {
        id: flow_id,
        name: "Flow".to_string(),
        value: GraphLiteral::new::<F32Type>(1.0),
    });

    ExternalVariables {
        size: size_id,
        opacity: opacity_id,
        flow: flow_id,
        storage: Arc::new(ext_vars),
    }
}

fn graph_resources<Data: GraphData>(
    node_registry: Arc<GraphNodeRegistry<Data>>,
    external_vars: Arc<GraphExternalVariableStorage>,
) -> GraphResources<Data> {
    GraphResources {
        type_registry: BRUSH_GRAPH_TYPES.clone(),
        node_registry,
        textures: lapiz_shader_graph::graph::texture::ASSET_GRAPH_TEXTURE_STORAGE.clone(),
        functions: lapiz_shader_graph::graph::function::ASSET_GRAPH_FUNCTION_STORAGE.clone(),
        external_vars,
    }
}
