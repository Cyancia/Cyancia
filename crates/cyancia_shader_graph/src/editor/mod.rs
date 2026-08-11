use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
    time::Instant,
};

use iced_core::{
    Background, Border, Clipboard, Color, Element, Event, Layout, Length, Point, Shell, Size,
    Vector,
    border::Radius,
    gradient::ColorStop,
    keyboard::{self, key},
    layout::{self, Limits, Node},
    mouse::{self, Interaction},
    overlay,
    renderer::{self, Quad},
    widget::{Operation, Tree, tree},
};
use iced_graphics::{
    geometry::{self, Frame, Stroke},
    gradient::Linear,
};
use iced_widget::{
    button, column, container,
    core::{Rectangle, Widget, mouse::Cursor},
    overlay::menu,
    row, stack, text,
};
use indexmap::IndexMap;
use uuid::Uuid;

use crate::{
    GraphRenderer, GraphTheme,
    editor::slot::{GraphSlotId, GraphSlotPinPositionCollection},
    graph::{
        Graph, GraphData, GraphResources,
        node::{ErasedGraphNodeMessage, GraphNodeData, GraphNodeId},
        slot::{GraphInputSlotId, GraphOutputSlotId, GraphSlots},
    },
};

pub mod slot;

pub const NODE_WIDTH: f32 = 200.0;
const NODE_BORDER_RADIUS: f32 = 5.0;

#[derive(Default)]
pub struct GraphEditorState {
    pub path: Vec<GraphEditorPathComponent>,
}

impl GraphEditorState {
    pub fn update<Data: GraphData>(
        &mut self,
        graph: &mut Graph<Data>,
        message: GraphEditorMessage,
    ) {
        match message {
            GraphEditorMessage::Graph(message) => {
                let target = self.resolve_subgraph_mut(graph);
                target.update(message);
            }
            GraphEditorMessage::Editor(GraphEditorEditorMessage::EnterSubgraph(
                comp,
                _snapshot,
            )) => {
                self.path.push(comp);
            }
            GraphEditorMessage::Editor(GraphEditorEditorMessage::BackToSubgraphOrMain(
                maybe_index,
            )) => {
                if let Some(index) = maybe_index {
                    self.path.truncate(index + 1);
                } else {
                    self.path.pop();
                }
            }
        }
    }

    pub fn resolve_subgraph<'a, Data: GraphData>(&self, main: &'a Graph<Data>) -> &'a Graph<Data> {
        let mut current = main;
        for comp in &self.path {
            let node = current.get_node(&comp.node_id).unwrap();
            current = node
                .data
                .subgraphs()
                .into_iter()
                .nth(comp.subgraph_index)
                .unwrap();
        }
        current
    }

    pub fn resolve_subgraph_mut<'a, Data: GraphData>(
        &self,
        main: &'a mut Graph<Data>,
    ) -> &'a mut Graph<Data> {
        let mut current = main;
        for comp in &self.path {
            let node = current.get_node_mut(&comp.node_id).unwrap();
            let subgraph = node
                .data
                .subgraphs_mut()
                .into_iter()
                .nth(comp.subgraph_index)
                .unwrap();
            current = subgraph;
        }
        current
    }
}

pub struct GraphEditor<'a, Data: GraphData> {
    graph: &'a Graph<Data>,
    editor_state: &'a GraphEditorState,
}

impl<'a, Data: GraphData> GraphEditor<'a, Data> {
    pub fn new(graph: &'a Graph<Data>, editor_state: &'a GraphEditorState) -> Self {
        Self {
            graph,
            editor_state,
        }
    }
}

impl<'a, Data: GraphData> From<GraphEditor<'a, Data>>
    for Element<'a, GraphEditorMessage, GraphTheme, GraphRenderer>
{
    fn from(value: GraphEditor<'a, Data>) -> Self {
        let GraphEditor {
            graph,
            editor_state,
        } = value;
        let editor_view =
            GraphEditorView::new(editor_state.resolve_subgraph(graph), editor_state, true);

        let mut cur_graph = graph;
        let subgraph_path = editor_state.path.iter().enumerate().map(|(index, comp)| {
            let node = cur_graph.get_node(&comp.node_id).unwrap();
            cur_graph = node
                .data
                .subgraphs()
                .into_iter()
                .nth(comp.subgraph_index)
                .unwrap();
            button(node.data.name())
                .on_press(GraphEditorMessage::Editor(
                    GraphEditorEditorMessage::BackToSubgraphOrMain(Some(index)),
                ))
                .into()
        });
        let main_graph_path = button("Main Graph").on_press(GraphEditorMessage::Editor(
            GraphEditorEditorMessage::BackToSubgraphOrMain(None),
        ));
        let path_breadcrumb = row![main_graph_path].extend(subgraph_path);
        stack!(editor_view, path_breadcrumb).into()
    }
}

#[derive(Debug, Clone)]
pub struct GraphEditorSnapshot {
    pub translation: Vector,
    pub selected_nodes: Vec<GraphNodeId>,
}

#[derive(Debug, Clone)]
pub enum GraphEditorMessage {
    Graph(GraphEditorGraphMessage),
    Editor(GraphEditorEditorMessage),
}

#[derive(Debug, Clone)]
pub enum GraphEditorGraphMessage {
    NodeCreateRequest(Point, &'static str, GraphNodeId),
    NodeMoveRequest(Point, GraphNodeId),
    NodeDeleteRequest(GraphNodeId),
    EdgeCreateRequest(GraphOutputSlotId, GraphInputSlotId),
    EdgeRemoveRequest(GraphInputSlotId),
    NodeUpdate(ErasedGraphNodeMessage),
    Format(HashMap<GraphNodeId, Rectangle>, HashSet<GraphNodeId>),
}

#[derive(Debug, Clone)]
pub enum GraphEditorEditorMessage {
    EnterSubgraph(GraphEditorPathComponent, GraphEditorSnapshot),
    BackToSubgraphOrMain(Option<usize>),
}

impl<Data: GraphData> Graph<Data> {
    pub fn update(&mut self, message: GraphEditorGraphMessage) {
        match message {
            GraphEditorGraphMessage::NodeCreateRequest(position, name, node_id) => {
                let node = self.resources.node_registry.get(name).unwrap();
                self.insert_boxed_node(node_id, position, node);
            }
            GraphEditorGraphMessage::NodeMoveRequest(position, id) => {
                self.get_node_mut(&id).unwrap().position = position;
            }
            GraphEditorGraphMessage::NodeDeleteRequest(id) => self.delete_node(&id),
            GraphEditorGraphMessage::EdgeCreateRequest(from, to) => {
                self.connect_slots(from, to);
            }
            GraphEditorGraphMessage::EdgeRemoveRequest(to) => self.disconnect_slot(to),
            GraphEditorGraphMessage::NodeUpdate(message) => self.update_node(message),
            GraphEditorGraphMessage::Format(bounds, selected) => self.format(&bounds, &selected),
        }
    }
}

pub struct GraphEditorView<'a, Data: GraphData> {
    graph: DrawableGraph,
    node_creation_menu_items: Vec<NodeCreationMenuItem>,
    node_creation_menu_class: <GraphTheme as menu::Catalog>::Class<'a>,
    snapshot: Option<GraphEditorSnapshot>,
    subgraphs: HashMap<GraphNodeId, Vec<&'a Graph<Data>>>,
    _state: &'a GraphEditorState,
}

impl<'a, Data: GraphData> GraphEditorView<'a, Data> {
    pub fn new(graph: &'a Graph<Data>, state: &'a GraphEditorState, is_dark: bool) -> Self {
        Self {
            graph: DrawableGraph::new(graph, is_dark),
            node_creation_menu_items: graph
                .resources
                .node_registry
                .all()
                .keys()
                .map(|title| NodeCreationMenuItem { node_title: title })
                .collect(),
            node_creation_menu_class: <GraphTheme as menu::Catalog>::default(),
            _state: state,
            snapshot: None,
            subgraphs: graph
                .nodes
                .iter()
                .filter_map(|(node_id, node)| {
                    let subgraphs = node.data.subgraphs();
                    if subgraphs.is_empty() {
                        None
                    } else {
                        Some((*node_id, subgraphs))
                    }
                })
                .collect(),
        }
    }

    pub fn recover(mut self, snapshot: GraphEditorSnapshot) -> Self {
        self.snapshot = Some(snapshot);
        self
    }
}

#[derive(Debug, Clone)]
pub struct GraphEditorPathComponent {
    pub node_id: GraphNodeId,
    pub subgraph_index: usize,
}

#[derive(Clone)]
pub struct NodeCreationMenuItem {
    pub node_title: &'static str,
}

impl std::fmt::Display for NodeCreationMenuItem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.node_title)
    }
}

pub struct GraphNodeStyle {
    pub background: Background,
    pub padding: f32,
    pub line_height: f32,
    pub line_spacing: f32,
}

pub struct DrawableGraph {
    pub nodes: IndexMap<GraphNodeId, DrawableNode>,
    pub slots: HashMap<GraphSlotId, SlotData>,
    pub edges: HashMap<GraphInputSlotId, DrawableEdge>,
    pub vert_in_loop: HashSet<GraphNodeId>,
}

impl DrawableGraph {
    pub fn new<Data: GraphData>(graph: &Graph<Data>, is_dark: bool) -> Self {
        let mut nodes = IndexMap::with_capacity(graph.nodes.len());
        let mut node_indices = HashMap::with_capacity(graph.nodes.len());
        for (index, (id, node)) in graph.nodes.iter().enumerate() {
            nodes.insert(
                *id,
                DrawableNode::new(*id, node, &graph.slots, graph.resources(), is_dark),
            );
            node_indices.insert(*id, index);
        }

        let edges = graph
            .slots
            .inputs
            .iter()
            .filter_map(|(to, to_slot)| {
                let from = graph.slots.inputs.get(to)?.connected?;
                let from_slot = graph.slots.outputs.get(&from)?;

                let from_color = from_slot.data_ty.color(is_dark);
                let to_color = to_slot.data.ty().color(is_dark);
                let style = if from_color == to_color {
                    geometry::Style::Solid(from_color)
                } else {
                    let g = Linear::new(Point::new(0.0, 0.0), Point::new(1000.0, 1000.0))
                        .add_stops([
                            ColorStop {
                                offset: 0.0,
                                color: from_color,
                            },
                            ColorStop {
                                offset: 1.0,
                                color: to_color,
                            },
                        ]);
                    geometry::Style::Gradient(g.into())
                };

                Some((*to, DrawableEdge { from, style }))
            })
            .collect();

        let slots = graph
            .slots
            .inputs
            .iter()
            .map(|(id, slot)| {
                (
                    (*id).into(),
                    SlotData {
                        color: slot.data.ty().color(is_dark),
                    },
                )
            })
            .chain(graph.slots.outputs.iter().map(|(id, slot)| {
                (
                    (*id).into(),
                    SlotData {
                        color: slot.data_ty.color(is_dark),
                    },
                )
            }))
            .collect();

        Self {
            nodes,
            edges,
            slots,
            vert_in_loop: graph.find_loops().into_iter().flatten().collect(),
        }
    }
}

pub struct SlotData {
    pub color: Color,
}

pub struct DrawableEdge {
    from: GraphOutputSlotId,
    style: geometry::Style,
}

pub struct DrawableNode {
    pub node_id: GraphNodeId,
    pub position: Point,
    pub widget: Element<'static, GraphEditorMessage, GraphTheme, GraphRenderer>,
    pub input_slots: Arc<[GraphInputSlotId]>,
    pub output_slots: Arc<[GraphOutputSlotId]>,
}

impl DrawableNode {
    pub fn new<Data: GraphData>(
        node_id: GraphNodeId,
        node: &GraphNodeData<Data>,
        slots: &GraphSlots,
        resources: &GraphResources<Data>,
        is_dark: bool,
    ) -> Self {
        let header_color = node.data.header_color(is_dark);
        let header = container(text(node.data.name()))
            .style(move |_| container::Style {
                background: Some(header_color.into()),
                border: Border {
                    radius: Radius {
                        top_left: NODE_BORDER_RADIUS,
                        top_right: NODE_BORDER_RADIUS,
                        ..Default::default()
                    },
                    ..Default::default()
                },
                ..Default::default()
            })
            .width(Length::Fill)
            .padding(5);

        let widget = container(
            column![
                header,
                node.view(node_id, slots, resources, is_dark)
                    .map(|m| GraphEditorMessage::Graph(GraphEditorGraphMessage::NodeUpdate(m))),
            ]
            .width(NODE_WIDTH),
        )
        .style(|t| container::Style {
            background: Some(t.extended_palette().background.strong.color.into()),
            border: Border::default().rounded(NODE_BORDER_RADIUS),
            ..Default::default()
        });

        Self {
            node_id,
            position: node.position,
            widget: Element::new(widget),
            input_slots: node.inputs.clone(),
            output_slots: node.outputs.clone(),
        }
    }
}

impl<'a, Data: GraphData> Widget<GraphEditorMessage, GraphTheme, GraphRenderer>
    for GraphEditorView<'a, Data>
{
    fn children(&self) -> Vec<Tree> {
        self.graph
            .nodes
            .values()
            .map(|node| Tree::new(&node.widget))
            .collect()
    }

    fn diff(&self, tree: &mut Tree) {
        tree.diff_children(
            &self
                .graph
                .nodes
                .values()
                .map(|n| &n.widget)
                .collect::<Vec<_>>(),
        );
    }

    fn size(&self) -> Size<Length> {
        Size::new(Length::Fill, Length::Fill)
    }

    fn state(&self) -> tree::State {
        tree::State::new(State::default())
    }

    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &GraphRenderer,
        limits: &layout::Limits,
    ) -> Node {
        let state = tree.state.downcast_mut::<State>();
        state.node_bounds.clear();

        let children = self
            .graph
            .nodes
            .values_mut()
            .zip(&mut tree.children)
            .map(|(node, tree)| {
                let layout = node
                    .widget
                    .as_widget_mut()
                    .layout(tree, renderer, &Limits::NONE)
                    .translate(Vector::new(node.position.x, node.position.y))
                    .translate(state.view_translation);
                state.node_bounds.insert(node.node_id, layout.bounds());
                layout
            })
            .collect();
        Node::with_children(
            limits.resolve(Length::Fill, Length::Fill, Size::ZERO),
            children,
        )
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &GraphRenderer,
        operation: &mut dyn Operation,
    ) {
        operation.container(None, layout.bounds());
        operation.traverse(&mut |operation| {
            self.graph
                .nodes
                .values_mut()
                .zip(&mut tree.children)
                .zip(layout.children())
                .for_each(|((child, state), layout)| {
                    child
                        .widget
                        .as_widget_mut()
                        .operate(state, layout, renderer, operation);
                });
        });
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &GraphRenderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, GraphEditorMessage>,
        viewport: &Rectangle,
    ) {
        let state = tree.state.downcast_mut::<State>();
        if let Event::Keyboard(keyboard::Event::ModifiersChanged(modifiers)) = event {
            state.keyboard_modifiers = *modifiers;
        }
        state.slot_pins.clear();
        let mut messages = Vec::new();
        let mut children_shell = Shell::new(&mut messages);
        for ((child, tree), layout) in self
            .graph
            .nodes
            .values_mut()
            .zip(&mut tree.children)
            .zip(layout.children())
        {
            child.widget.as_widget_mut().update(
                tree,
                event,
                layout,
                cursor,
                renderer,
                clipboard,
                &mut children_shell,
                viewport,
            );

            child
                .widget
                .as_widget_mut()
                .operate(tree, layout, renderer, &mut state.slot_pins);
        }
        shell.merge(children_shell, |m| m);

        if shell.is_event_captured() {
            dbg!();
            return;
        }

        const SLOT_PIN_SNAP: f32 = 3.0 * 3.0;
        match event {
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Right)) => {
                let Some(cursor) = cursor.position_over(layout.bounds()) else {
                    return;
                };
                state.node_creation_menu.position = Some(cursor);
                shell.capture_event();
                shell.request_redraw();
            }
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Middle)) => {
                let Some(cursor) = cursor.position_over(layout.bounds()) else {
                    return;
                };

                state.interaction = InteractionState::ViewDragging {
                    cursor_origin: cursor,
                    translation_origin: state.view_translation,
                };
                shell.capture_event();
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Middle)) => {
                if matches!(state.interaction, InteractionState::ViewDragging { .. }) {
                    state.interaction = InteractionState::Idle;
                    shell.capture_event();
                }
            }
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                let Some(cursor) = cursor.position_over(layout.bounds()) else {
                    return;
                };

                for (slot_id, slot_pos) in state.slot_pins.all() {
                    let d = slot_pos.distance(cursor);
                    if d > SLOT_PIN_SNAP {
                        continue;
                    }

                    let resolved_source = match slot_id {
                        GraphSlotId::Input(id) => {
                            shell.publish(GraphEditorMessage::Graph(
                                GraphEditorGraphMessage::EdgeRemoveRequest(*id),
                            ));

                            self.graph
                                .edges
                                .get(id)
                                .map(|e| GraphSlotId::Output(e.from))
                                .unwrap_or(GraphSlotId::Input(*id))
                        }
                        GraphSlotId::Output(id) => GraphSlotId::Output(*id),
                    };
                    let Some(slot_data) = self.graph.slots.get(slot_id) else {
                        continue;
                    };

                    state.interaction = InteractionState::EdgeConnecting {
                        resolved_source,
                        color: slot_data.color,
                    };
                    shell.capture_event();
                    return;
                }

                for (node_index, node_layout) in layout.children().enumerate() {
                    if !node_layout.bounds().contains(cursor) {
                        continue;
                    }

                    let node_id = self.graph.nodes[node_index].node_id;
                    if let Some(last_click_on_node) = &state.last_click_on_node
                        && last_click_on_node.elapsed().as_secs_f32() < 0.2
                        && let Some(_) = self.subgraphs.get(&node_id)
                    {
                        shell.publish(GraphEditorMessage::Editor(
                            GraphEditorEditorMessage::EnterSubgraph(
                                GraphEditorPathComponent {
                                    node_id,
                                    // TODO add support for multiple subgraphs
                                    subgraph_index: 0,
                                },
                                GraphEditorSnapshot {
                                    translation: state.view_translation,
                                    selected_nodes: state.selected_nodes.iter().copied().collect(),
                                },
                            ),
                        ));
                        return;
                    }
                    state.last_click_on_node = Some(Instant::now());

                    if state.selected_nodes.is_empty() {
                        state.selected_nodes.insert(node_id);
                    } else if state.keyboard_modifiers.control() {
                        if !state.selected_nodes.remove(&node_id) {
                            state.selected_nodes.insert(node_id);
                        }
                    } else if !state.selected_nodes.contains(&node_id) {
                        state.selected_nodes.clear();
                        state.selected_nodes.insert(node_id);
                    }
                    state.interaction = InteractionState::NodeDragging {
                        cursor_origin: cursor,
                        node_origin: state
                            .selected_nodes
                            .iter()
                            .filter_map(|id| {
                                self.graph.nodes.get_index_of(id).map(|index| (id, index))
                            })
                            .map(|(id, index)| (*id, layout.child(index).position()))
                            .collect(),
                        skip_next_release: false,
                    };
                    shell.request_redraw();
                    shell.capture_event();
                    return;
                }

                let mode = if state.keyboard_modifiers.shift() {
                    MarqueeMode::Add
                } else {
                    state.selected_nodes.clear();
                    MarqueeMode::Replace
                };
                state.interaction = InteractionState::SelectionDragging {
                    cursor_origin: cursor,
                    originally_selected: state.selected_nodes.clone(),
                    mode,
                };
                shell.capture_event();
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                match std::mem::take(&mut state.interaction) {
                    InteractionState::NodeDragging {
                        cursor_origin,
                        node_origin,
                        skip_next_release,
                    } => {
                        if skip_next_release {
                            state.interaction = InteractionState::NodeDragging {
                                cursor_origin,
                                node_origin,
                                skip_next_release: false,
                            };
                        }
                        shell.capture_event();
                    }
                    InteractionState::EdgeConnecting {
                        resolved_source, ..
                    } => {
                        let mut found = None;
                        for (slot_id, slot_pos) in state.slot_pins.all() {
                            let cursor = cursor.position().unwrap();
                            if slot_pos.distance(cursor) < SLOT_PIN_SNAP {
                                found = Some(*slot_id);
                                break;
                            }
                        }

                        if let Some(end) = found {
                            match (resolved_source, end) {
                                (GraphSlotId::Input(to), GraphSlotId::Output(from))
                                | (GraphSlotId::Output(from), GraphSlotId::Input(to)) => {
                                    shell.publish(GraphEditorMessage::Graph(
                                        GraphEditorGraphMessage::EdgeCreateRequest(from, to),
                                    ));
                                }
                                _ => {}
                            }
                        }
                        shell.capture_event();
                        shell.request_redraw();
                    }
                    InteractionState::SelectionDragging {
                        cursor_origin,
                        originally_selected,
                        mode,
                    } => {
                        let Some(cursor) = cursor.position() else {
                            state.interaction = InteractionState::SelectionDragging {
                                cursor_origin,
                                originally_selected,
                                mode,
                            };
                            return;
                        };
                        let selection_rect = Rectangle {
                            x: cursor_origin.x.min(cursor.x),
                            y: cursor_origin.y.min(cursor.y),
                            width: (cursor_origin.x - cursor.x).abs(),
                            height: (cursor_origin.y - cursor.y).abs(),
                        };
                        state.selected_nodes = match mode {
                            MarqueeMode::Replace => HashSet::new(),
                            MarqueeMode::Add => originally_selected,
                        };
                        for (node, layout) in self.graph.nodes.keys().zip(layout.children()) {
                            if selection_rect.intersects(&layout.bounds()) {
                                state.selected_nodes.insert(*node);
                            }
                        }
                        shell.request_redraw();
                        shell.capture_event();
                    }
                    interaction => state.interaction = interaction,
                }
            }
            Event::Mouse(mouse::Event::CursorMoved { .. }) => match &state.interaction {
                InteractionState::Idle => {}
                InteractionState::EdgeConnecting { .. } => {
                    shell.request_redraw();
                    shell.capture_event();
                }
                InteractionState::NodeDragging {
                    cursor_origin,
                    node_origin,
                    ..
                } => {
                    let Some(cursor) = cursor.position() else {
                        return;
                    };
                    for selected in &state.selected_nodes {
                        if let Some(node_origin) = node_origin.get(selected) {
                            shell.publish(GraphEditorMessage::Graph(
                                GraphEditorGraphMessage::NodeMoveRequest(
                                    *node_origin + (cursor - *cursor_origin)
                                        - Vector::new(layout.position().x, layout.position().y)
                                        - state.view_translation,
                                    *selected,
                                ),
                            ));
                        }
                    }
                    shell.capture_event();
                }
                InteractionState::SelectionDragging {
                    cursor_origin,
                    originally_selected,
                    mode,
                } => {
                    let Some(cursor) = cursor.position() else {
                        return;
                    };
                    let selection_rect = Rectangle {
                        x: cursor_origin.x.min(cursor.x),
                        y: cursor_origin.y.min(cursor.y),
                        width: (cursor_origin.x - cursor.x).abs(),
                        height: (cursor_origin.y - cursor.y).abs(),
                    };
                    state.selected_nodes = match mode {
                        MarqueeMode::Replace => HashSet::new(),
                        MarqueeMode::Add => originally_selected.clone(),
                    };
                    for (node, layout) in self.graph.nodes.keys().zip(layout.children()) {
                        if selection_rect.intersects(&layout.bounds()) {
                            state.selected_nodes.insert(*node);
                        }
                    }
                    shell.request_redraw();
                    shell.capture_event();
                }
                InteractionState::ViewDragging {
                    cursor_origin,
                    translation_origin,
                } => {
                    let Some(cursor) = cursor.position() else {
                        return;
                    };
                    state.view_translation = *translation_origin + (cursor - *cursor_origin);
                    shell.capture_event();
                    shell.invalidate_layout();
                    shell.request_redraw();
                }
            },
            Event::Keyboard(keyboard::Event::KeyPressed {
                physical_key,
                modifiers,
                repeat,
                ..
            }) => {
                if *repeat {
                    return;
                }
                let key::Physical::Code(key) = *physical_key else {
                    return;
                };

                // TODO: make them configurable
                match key {
                    key::Code::Delete => {
                        for node_id in state.selected_nodes.drain() {
                            shell.publish(GraphEditorMessage::Graph(
                                GraphEditorGraphMessage::NodeDeleteRequest(node_id),
                            ));
                        }
                        shell.capture_event();
                        shell.request_redraw();
                    }
                    key::Code::KeyA if modifiers.contains(keyboard::Modifiers::SHIFT) => {
                        state.node_creation_menu.position = cursor.position();
                        shell.capture_event();
                        shell.request_redraw();
                    }
                    key::Code::KeyF if modifiers.shift() && modifiers.alt() => {
                        shell.publish(GraphEditorMessage::Graph(GraphEditorGraphMessage::Format(
                            state.node_bounds.clone(),
                            state.selected_nodes.clone(),
                        )));
                        shell.capture_event();
                        shell.request_redraw();
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: Cursor,
        viewport: &Rectangle,
        renderer: &GraphRenderer,
    ) -> Interaction {
        let state = tree.state.downcast_ref::<State>();

        if matches!(state.interaction, InteractionState::NodeDragging { .. }) {
            mouse::Interaction::Grabbing
        } else {
            self.graph
                .nodes
                .values()
                .zip(&tree.children)
                .zip(layout.children())
                .map(|((child, tree), layout)| {
                    child
                        .widget
                        .as_widget()
                        .mouse_interaction(tree, layout, cursor, viewport, renderer)
                })
                .max()
                .unwrap_or_default()
        }
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut GraphRenderer,
        theme: &iced_core::Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: Cursor,
        viewport: &Rectangle,
    ) {
        let state = tree.state.downcast_ref::<State>();
        {
            use iced_core::Renderer;

            renderer.fill_quad(
                Quad {
                    bounds: layout.bounds(),
                    ..Default::default()
                },
                theme.extended_palette().background.base.color,
            );
        }

        let mut frame = Frame::with_bounds(renderer, layout.bounds());
        for (to, edge) in &self.graph.edges {
            let from_pos = state.slot_pins.get_output(&edge.from);
            let to_pos = state.slot_pins.get_input(to);
            if let (Some(from_pos), Some(to_pos)) = (from_pos, to_pos) {
                let style = match edge.style {
                    geometry::Style::Solid(color) => color.into(),
                    geometry::Style::Gradient(gradient) => match gradient {
                        geometry::Gradient::Linear(linear) => geometry::Style::Gradient(
                            Linear {
                                start: *from_pos,
                                end: *to_pos,
                                ..linear
                            }
                            .into(),
                        ),
                    },
                };

                frame.stroke(
                    &geometry::Path::line(*from_pos, *to_pos),
                    Stroke {
                        style,
                        width: 2.0,
                        ..Default::default()
                    },
                );
            }
        }

        {
            use iced_core::Renderer;

            renderer.with_layer(layout.bounds(), |renderer| {
                use iced_graphics::geometry::Renderer;
                renderer.draw_geometry(frame.into_geometry());
            });
        }

        for ((child, node_tree), node_layout) in self
            .graph
            .nodes
            .values()
            .zip(&tree.children)
            .zip(layout.children())
            .filter(|(_, layout)| layout.bounds().intersects(viewport))
        {
            use iced_core::Renderer;
            renderer.with_layer(layout.bounds(), |renderer| {
                if state.selected_nodes.contains(&child.node_id) {
                    renderer.fill_quad(
                        Quad {
                            bounds: node_layout.bounds().expand(2.0),
                            border: Border::default().rounded(NODE_BORDER_RADIUS),
                            ..Default::default()
                        },
                        Color::WHITE,
                    );
                }
                child.widget.as_widget().draw(
                    node_tree,
                    renderer,
                    theme,
                    style,
                    node_layout,
                    cursor,
                    viewport,
                );
                if self.graph.vert_in_loop.contains(&child.node_id) {
                    renderer.fill_quad(
                        Quad {
                            bounds: node_layout.bounds(),
                            border: Border::default().rounded(NODE_BORDER_RADIUS),
                            ..Default::default()
                        },
                        Color::from_rgb8(255, 0, 0).scale_alpha(0.3),
                    );
                }
            });
        }

        if let (
            InteractionState::EdgeConnecting {
                resolved_source,
                color,
            },
            Some(cursor_pos),
        ) = (&state.interaction, cursor.position())
            && let Some(start_pos) = state.slot_pins.get(resolved_source)
        {
            let mut frame = Frame::with_bounds(renderer, layout.bounds());
            frame.stroke(
                &geometry::Path::line(*start_pos, cursor_pos),
                Stroke {
                    style: (*color).into(),
                    width: 2.0,
                    ..Default::default()
                },
            );

            use iced_core::Renderer;
            renderer.with_layer(layout.bounds(), |renderer| {
                use iced_graphics::geometry::Renderer;
                renderer.draw_geometry(frame.into_geometry());
            });
        };

        if let InteractionState::SelectionDragging { cursor_origin, .. } = &state.interaction {
            let Some(cursor_pos) = cursor.position() else {
                return;
            };
            use iced_core::Renderer;
            renderer.with_layer(layout.bounds(), |renderer| {
                renderer.fill_quad(
                    Quad {
                        bounds: Rectangle {
                            x: cursor_origin.x.min(cursor_pos.x),
                            y: cursor_origin.y.min(cursor_pos.y),
                            width: (cursor_origin.x - cursor_pos.x).abs(),
                            height: (cursor_origin.y - cursor_pos.y).abs(),
                        },
                        border: Border::default().width(2.0).color(
                            theme
                                .extended_palette()
                                .primary
                                .strong
                                .color
                                .scale_alpha(0.5),
                        ),
                        ..Default::default()
                    },
                    theme.extended_palette().primary.base.color.scale_alpha(0.3),
                );
            });
        }
    }

    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout<'b>,
        renderer: &GraphRenderer,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<overlay::Element<'b, GraphEditorMessage, GraphTheme, GraphRenderer>> {
        for ((child, tree), layout) in self
            .graph
            .nodes
            .values_mut()
            .zip(&mut tree.children)
            .zip(layout.children())
        {
            if let Some(overlay) =
                child
                    .widget
                    .as_widget_mut()
                    .overlay(tree, layout, renderer, viewport, translation)
            {
                return Some(overlay);
            }
        }

        let state = tree.state.downcast_mut::<State>();
        if let Some(menu_position) = state.node_creation_menu.position {
            let menu = menu::Menu::new(
                &mut state.node_creation_menu.state,
                &self.node_creation_menu_items,
                &mut state.node_creation_menu.hovered,
                |name| {
                    let position = state.node_creation_menu.position.take().unwrap();
                    let node_id = GraphNodeId::new(Uuid::new_v4());
                    state.selected_nodes.clear();
                    state.selected_nodes.insert(node_id);
                    state.interaction = InteractionState::NodeDragging {
                        cursor_origin: position,
                        node_origin: HashMap::from([(node_id, position)]),
                        skip_next_release: true,
                    };
                    GraphEditorMessage::Graph(GraphEditorGraphMessage::NodeCreateRequest(
                        position - state.view_translation,
                        name.node_title,
                        node_id,
                    ))
                },
                None,
                &self.node_creation_menu_class,
            )
            .width(200.0)
            .padding(2);

            return Some(menu.overlay(menu_position, *viewport, 0.0, Length::Shrink));
        }

        None
    }
}

impl<'a, Data: GraphData> From<GraphEditorView<'a, Data>>
    for Element<'a, GraphEditorMessage, GraphTheme, GraphRenderer>
{
    fn from(value: GraphEditorView<'a, Data>) -> Self {
        Element::new(value)
    }
}

#[derive(Default)]
struct State {
    view_translation: Vector,
    keyboard_modifiers: keyboard::Modifiers,

    node_creation_menu: NodeCreationMenuState,
    last_click_on_node: Option<Instant>,
    selected_nodes: HashSet<GraphNodeId>,
    interaction: InteractionState,
    node_bounds: HashMap<GraphNodeId, Rectangle>,
    slot_pins: GraphSlotPinPositionCollection,
}

#[derive(Default)]
struct NodeCreationMenuState {
    position: Option<Point>,
    state: menu::State,
    hovered: Option<usize>,
}

#[derive(Default)]
enum InteractionState {
    #[default]
    Idle,
    NodeDragging {
        cursor_origin: Point,
        node_origin: HashMap<GraphNodeId, Point>,
        skip_next_release: bool,
    },
    EdgeConnecting {
        resolved_source: GraphSlotId,
        color: Color,
    },
    SelectionDragging {
        cursor_origin: Point,
        originally_selected: HashSet<GraphNodeId>,
        mode: MarqueeMode,
    },
    ViewDragging {
        cursor_origin: Point,
        translation_origin: Vector,
    },
}

#[derive(Clone, Copy)]
enum MarqueeMode {
    Replace,
    Add,
}
