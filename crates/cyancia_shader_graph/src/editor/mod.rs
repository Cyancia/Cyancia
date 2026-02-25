use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

use iced_core::{
    Background, Border, Clipboard, Color, Element, Event, Layout, Length, Point, Shadow, Shell,
    Size, Vector,
    alignment::Horizontal,
    border::{self, Radius},
    gradient::ColorStop,
    keyboard::{self, key},
    layout::{self, Limits, Node},
    mouse::{self, Interaction},
    overlay,
    renderer::{self, Quad},
    widget::{Operation, Tree, tree},
};
use iced_graphics::{
    futures::backend::default,
    geometry::{self, Frame, Stroke},
    gradient::Linear,
};
use iced_widget::{
    Renderer, column, container,
    core::{Rectangle, Widget, mouse::Cursor},
    overlay::menu,
    row, space, text,
};
use indexmap::IndexMap;

use crate::{
    GraphRenderer, GraphTheme,
    editor::slot::{
        GraphSlotId, GraphSlotPinPositionCollection, SlotSide, empty_slot, output_slot, valued_slot,
    },
    graph::{
        Graph, GraphDynamicInstancesStorage,
        node::{ErasedGraphNode, ErasedGraphNodeMessage, GraphNodeData, GraphNodeId},
        slot::{
            GraphInputSlotData, GraphInputSlotId, GraphOutputSlotData, GraphOutputSlotId,
            GraphSlots,
        },
    },
};

pub mod slot;

const NODE_WIDTH: f32 = 170.0;
const NODE_BORDER_RADIUS: f32 = 5.0;

pub enum GraphViewMessage {
    NodeCreateRequest(Point, Box<dyn ErasedGraphNode>),
    NodeMoveRequest(Point, GraphNodeId),
    NodeDeleteRequest(GraphNodeId),
    EdgeCreateRequest(GraphOutputSlotId, GraphInputSlotId),
    EdgeRemoveRequest(GraphInputSlotId),
    NodeUpdate(ErasedGraphNodeMessage),
}

impl std::fmt::Debug for GraphViewMessage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NodeCreateRequest(arg0, arg1) => {
                f.debug_tuple("NodeCreateRequest").field(arg0).finish()
            }
            Self::NodeMoveRequest(arg0, arg1) => f
                .debug_tuple("NodeMoveRequest")
                .field(arg0)
                .field(arg1)
                .finish(),
            Self::NodeDeleteRequest(arg0) => {
                f.debug_tuple("NodeDeleteRequest").field(arg0).finish()
            }
            Self::EdgeCreateRequest(arg0, arg1) => f
                .debug_tuple("EdgeCreateRequest")
                .field(arg0)
                .field(arg1)
                .finish(),
            Self::EdgeRemoveRequest(arg0) => {
                f.debug_tuple("EdgeRemoveRequest").field(arg0).finish()
            }
            Self::NodeUpdate(arg0) => f.debug_tuple("NodeUpdate").finish(),
        }
    }
}

// impl<'a, Message, Theme, Renderer> GraphSlotViewers<'a, Message, Theme, Renderer>
// where
//     Message: 'a,
//     Theme: text::Catalog + 'a,
//     Renderer: iced_core::Renderer + iced_core::text::Renderer + 'a,
// {
//     pub fn new() -> Self {
//         Self {
//             viewers: HashMap::new(),
//         }
//     }

//     pub fn register<V: GraphSlotViewer<'a, Message, Theme, Renderer> + 'static>(
//         &mut self,
//         viewer: V,
//     ) {
//         self.viewers.insert(viewer.type_name(), Box::new(viewer));
//     }

//     pub fn view_input(
//         &self,
//         id: InputSlotId,
//         slot: &GraphInputSlot,
//     ) -> Option<Element<'a, Message, Theme, Renderer>> {
//         let viewer = self.viewers.get(slot.value_type.type_name())?;
//         if slot.connected.is_some() {
//             Some(empty_slot(viewer.color(), slot.name, SlotSide::Left))
//         } else {
//             Some(
//                 viewer
//                     .view(slot.name, &slot.value, GraphSlotId::Input(id))
//                     .map(|widget| valued_slot(viewer.color(), slot.name, SlotSide::Left, widget))
//                     .unwrap_or_else(|| empty_slot(viewer.color(), slot.name, SlotSide::Left)),
//             )
//         }
//     }

//     pub fn view_output(
//         &self,
//         id: OutputSlotId,
//         slot: &GraphOutputSlot,
//     ) -> Option<Element<'a, Message, Theme, Renderer>> {
//         let viewer = self.viewers.get(slot.value_type.type_name())?;
//         Some(empty_slot(viewer.color(), slot.name, SlotSide::Right))
//     }
// }

pub struct GraphView<'a> {
    graph: DrawableGraph<'a>,
    storage: Arc<GraphDynamicInstancesStorage>,
    node_creation_menu_items: Vec<NodeCreationMenuItem>,
    node_creation_menu_class: <GraphTheme as menu::Catalog>::Class<'a>,
}

impl<'a> GraphView<'a> {
    pub fn new(graph: &'a Graph) -> Self {
        Self {
            graph: DrawableGraph::new(graph),
            storage: graph.storage().clone(),
            node_creation_menu_items: graph
                .storage()
                .nodes
                .all()
                .keys()
                .map(|title| NodeCreationMenuItem {
                    node_title: title.to_string(),
                })
                .collect(),
            node_creation_menu_class: <GraphTheme as menu::Catalog>::default(),
        }
    }
}

#[derive(Clone)]
pub struct NodeCreationMenuItem {
    pub node_title: String,
}

impl ToString for NodeCreationMenuItem {
    fn to_string(&self) -> String {
        self.node_title.clone()
    }
}

pub struct GraphNodeStyle {
    pub background: Background,
    pub padding: f32,
    pub line_height: f32,
    pub line_spacing: f32,
}

pub struct DrawableGraph<'a> {
    pub nodes: IndexMap<GraphNodeId, DrawableNode<'a>>,
    pub slots: HashMap<GraphSlotId, SlotData>,
    pub edges: HashMap<GraphInputSlotId, DrawableEdge>,
    pub vert_in_loop: HashSet<GraphNodeId>,
}

impl<'a> DrawableGraph<'a> {
    pub fn new(graph: &'a Graph) -> Self {
        let mut nodes = IndexMap::with_capacity(graph.nodes.len());
        let mut node_indices = HashMap::with_capacity(graph.nodes.len());
        for (index, (id, node)) in graph.nodes.iter().enumerate() {
            nodes.insert(
                *id,
                DrawableNode::new(*id, node, &graph.slots, graph.storage()),
            );
            node_indices.insert(*id, index);
        }

        let edges = graph
            .slots
            .inputs
            .iter()
            .filter_map(|(to, to_slot)| {
                let from = graph.slots.inputs.get(&to)?.connected?;
                let from_slot = graph.slots.outputs.get(&from)?;

                let from_color = from_slot.data_ty.color();
                let to_color = to_slot.data.ty().color();
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
                        color: slot.data.ty().color(),
                    },
                )
            })
            .chain(graph.slots.outputs.iter().map(|(id, slot)| {
                (
                    (*id).into(),
                    SlotData {
                        color: slot.data_ty.color(),
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

pub struct DrawableNode<'a> {
    pub node_id: GraphNodeId,
    pub position: Point,
    pub widget: Element<'a, GraphViewMessage, GraphTheme, GraphRenderer>,
    pub input_slots: Arc<[GraphInputSlotId]>,
    pub output_slots: Arc<[GraphOutputSlotId]>,
}

impl<'a> DrawableNode<'a> {
    pub fn new(
        node_id: GraphNodeId,
        node: &'a GraphNodeData,
        slots: &GraphSlots,
        storage: &GraphDynamicInstancesStorage,
    ) -> Self {
        // let inputs = node
        //     .inputs
        //     .iter()
        //     .filter_map(|slot_id| slots.inputs.get(slot_id).map(|slot| (slot_id, slot)))
        //     .filter_map(|(slot_id, slot)| match &slot.connected {
        //         Some(_) => Some(empty_slot(
        //             slot.data.ty().color(),
        //             slot.name,
        //             SlotSide::Left,
        //         )),
        //         None => match slot.slot_type {
        //             GraphSlotType::Normal => Some(valued_slot(
        //                 slot.data.ty().color(),
        //                 slot.name,
        //                 SlotSide::Left,
        //                 slot.data.ty().view_literal(*slot_id, &slot.data.value),
        //             )),
        //             GraphSlotType::Unconnectable => Some(valued_slot_unconnectable(
        //                 slot.name,
        //                 SlotSide::Left,
        //                 slot.data.ty().view_literal(*slot_id, &slot.data.value),
        //             )),
        //             GraphSlotType::Hidden => None,
        //         },
        //     });
        // let inputs = column(inputs).spacing(2);
        let header_color = node.data.header_color();
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
                column!(
                    node.view_inputs(node_id, slots, storage)
                        .map(GraphViewMessage::NodeUpdate),
                    row![
                        space().width(Length::Fill),
                        node.view_outputs(node_id, slots, storage)
                            .map(GraphViewMessage::NodeUpdate)
                    ],
                )
                .padding(2),
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

impl<'a> Widget<GraphViewMessage, GraphTheme, GraphRenderer> for GraphView<'a> {
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
        let state = tree.state.downcast_ref::<State>();
        let children = self
            .graph
            .nodes
            .values_mut()
            .zip(&mut tree.children)
            .map(|(node, tree)| {
                node.widget
                    .as_widget_mut()
                    .layout(tree, renderer, &Limits::NONE)
                    .translate(Vector::new(node.position.x, node.position.y))
                    .translate(state.view_translation)
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
        shell: &mut Shell<'_, GraphViewMessage>,
        viewport: &Rectangle,
    ) {
        let state = tree.state.downcast_mut::<State>();
        match event {
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                // We need to handle it before children, otherwise, if the drag is started on a interactable child,
                // the event will get captured and unable to be identified below, and will stuck.
                if let DragNodeState::Dragging { .. } = state.node_drag {
                    state.node_drag = DragNodeState::Idle;
                    shell.capture_event();
                    return;
                }

                state.node_creation_menu.position = None;
            }
            _ => {}
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
            return;
        }

        const SLOT_PIN_SNAP: f32 = 3.0 * 3.0;
        match event {
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Middle)) => {
                let Some(cursor) = cursor.position() else {
                    return;
                };

                state.view_drag = ViewDragState::Dragging {
                    cursor_origin: cursor,
                    translation_origin: state.view_translation,
                };
                shell.capture_event();
                return;
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Middle)) => {
                if let ViewDragState::Dragging { .. } = state.view_drag {
                    state.view_drag = ViewDragState::Idle;
                    shell.capture_event();
                    return;
                }
            }
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                let Some(cursor) = cursor.position() else {
                    return;
                };

                for (slot_id, slot_pos) in state.slot_pins.all() {
                    let d = slot_pos.distance(cursor);
                    if d < SLOT_PIN_SNAP {
                        let resolved_source = match slot_id {
                            GraphSlotId::Input(id) => {
                                shell.publish(GraphViewMessage::EdgeRemoveRequest(*id));

                                self.graph
                                    .edges
                                    .get(&(*id).into())
                                    .map(|e| GraphSlotId::Output(e.from))
                                    .unwrap_or(GraphSlotId::Input(*id))
                            }
                            GraphSlotId::Output(id) => GraphSlotId::Output(*id),
                        };
                        let Some(slot_data) = self.graph.slots.get(slot_id) else {
                            continue;
                        };

                        state.edge_connect = EdgeConnectState::Dragging {
                            resolved_source,
                            color: slot_data.color,
                        };
                        shell.capture_event();
                        return;
                    }
                }

                for (node_index, node_layout) in layout.children().enumerate() {
                    if node_layout.bounds().contains(cursor) {
                        let node_id = self.graph.nodes[node_index].node_id;
                        if !state.selection.selected_nodes.contains(&node_id) {
                            state.selection.selected_nodes.clear();
                            state.selection.selected_nodes.insert(node_id);
                        }
                        state.node_drag = DragNodeState::Dragging {
                            cursor_origin: cursor,
                            node_origin: state
                                .selection
                                .selected_nodes
                                .iter()
                                .filter_map(|id| {
                                    self.graph.nodes.get_index_of(id).map(|index| (id, index))
                                })
                                .map(|(id, index)| (*id, layout.child(index).position()))
                                .collect(),
                        };
                        shell.request_redraw();
                        shell.capture_event();
                        return;
                    }
                }

                state.selection.state = DragSelectionState::Dragging {
                    cursor_origin: cursor,
                };
                shell.capture_event();
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                if let DragNodeState::Dragging { .. } = &state.node_drag {
                    state.node_drag = DragNodeState::Idle;
                    shell.capture_event();
                    return;
                }

                if let EdgeConnectState::Dragging {
                    resolved_source,
                    color,
                } = &state.edge_connect
                {
                    let mut found = None;
                    for (slot_id, slot_pos) in state.slot_pins.all() {
                        let cursor = cursor.position().unwrap();
                        let d = slot_pos.distance(cursor);
                        if d < SLOT_PIN_SNAP {
                            found = Some(*slot_id);
                            break;
                        }
                    }

                    if let Some(end) = found {
                        match (*resolved_source, end) {
                            (GraphSlotId::Input(to), GraphSlotId::Output(from)) => {
                                shell.publish(GraphViewMessage::EdgeCreateRequest(from, to));
                            }
                            (GraphSlotId::Output(from), GraphSlotId::Input(to)) => {
                                shell.publish(GraphViewMessage::EdgeCreateRequest(from, to));
                            }
                            _ => {}
                        }
                    }
                    state.edge_connect = EdgeConnectState::Idle;
                    shell.capture_event();
                    shell.request_redraw();
                    return;
                }

                if let DragSelectionState::Dragging { cursor_origin } = state.selection.state {
                    state.selection.state = DragSelectionState::Idle;
                    let Some(cursor) = cursor.position() else {
                        return;
                    };
                    let selection_rect = Rectangle {
                        x: cursor_origin.x.min(cursor.x),
                        y: cursor_origin.y.min(cursor.y),
                        width: (cursor_origin.x - cursor.x).abs(),
                        height: (cursor_origin.y - cursor.y).abs(),
                    };

                    state.selection.selected_nodes.clear();
                    for (node, layout) in self.graph.nodes.keys().zip(layout.children()) {
                        if selection_rect.intersects(&layout.bounds()) {
                            state.selection.selected_nodes.insert(*node);
                        }
                    }

                    state.selection.state = DragSelectionState::Idle;
                    shell.request_redraw();
                    shell.capture_event();
                    return;
                }
            }
            Event::Mouse(mouse::Event::CursorMoved { .. }) => {
                match &state.edge_connect {
                    EdgeConnectState::Idle => {}
                    EdgeConnectState::Dragging { .. } => {
                        shell.request_redraw();
                        shell.capture_event();
                        return;
                    }
                }

                let Some(cursor) = cursor.position() else {
                    return;
                };

                match &state.node_drag {
                    DragNodeState::Idle => {}
                    DragNodeState::Dragging {
                        cursor_origin: origin,
                        node_origin,
                    } => {
                        for selected in &state.selection.selected_nodes {
                            if let Some(node_origin) = node_origin.get(selected) {
                                shell.publish(GraphViewMessage::NodeMoveRequest(
                                    *node_origin + (cursor - *origin)
                                        - Vector::new(layout.position().x, layout.position().y)
                                        - state.view_translation,
                                    *selected,
                                ));
                            }
                        }
                        shell.capture_event();
                        return;
                    }
                }

                match state.selection.state {
                    DragSelectionState::Idle => {}
                    DragSelectionState::Dragging { .. } => {
                        shell.request_redraw();
                        shell.capture_event();
                        return;
                    }
                }

                match &state.view_drag {
                    ViewDragState::Idle => {}
                    ViewDragState::Dragging {
                        cursor_origin,
                        translation_origin,
                    } => {
                        state.view_translation = *translation_origin + (cursor - *cursor_origin);
                        shell.capture_event();
                        shell.invalidate_layout();
                        shell.request_redraw();
                        return;
                    }
                }
            }
            Event::Keyboard(keyboard::Event::KeyPressed {
                key,
                modified_key,
                physical_key,
                location,
                modifiers,
                text,
                repeat,
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
                        for node_id in &state.selection.selected_nodes {
                            shell.publish(GraphViewMessage::NodeDeleteRequest(*node_id));
                        }
                    }
                    key::Code::KeyA if modifiers.contains(keyboard::Modifiers::SHIFT) => {
                        state.node_creation_menu.position = cursor.position();
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

        match state.node_drag {
            DragNodeState::Idle => self
                .graph
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
                .unwrap_or_default(),
            DragNodeState::Dragging { .. } => mouse::Interaction::Grabbing,
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
                if state.selection.selected_nodes.contains(&child.node_id) {
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
            EdgeConnectState::Dragging {
                resolved_source,
                color,
            },
            Some(cursor_pos),
        ) = (&state.edge_connect, cursor.position())
            && let Some(start_pos) = state.slot_pins.get(&resolved_source)
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

        if let DragSelectionState::Dragging { cursor_origin } = state.selection.state {
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
    ) -> Option<overlay::Element<'b, GraphViewMessage, GraphTheme, GraphRenderer>> {
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
                    let position = state.node_creation_menu.position.unwrap();
                    state.node_creation_menu.position = None;
                    GraphViewMessage::NodeCreateRequest(
                        position - state.view_translation,
                        self.storage.nodes.get_cloned(&name.node_title).unwrap(),
                    )
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

impl<'a> From<GraphView<'a>> for Element<'a, GraphViewMessage, GraphTheme, GraphRenderer> {
    fn from(value: GraphView<'a>) -> Self {
        Element::new(value)
    }
}

#[derive(Default)]
struct State {
    view_translation: Vector,

    node_creation_menu: NodeCreationMenuState,
    node_drag: DragNodeState,
    view_drag: ViewDragState,
    edge_connect: EdgeConnectState,
    selection: NodeSelectionState,
    slot_pins: GraphSlotPinPositionCollection,
}

#[derive(Default)]
struct NodeCreationMenuState {
    position: Option<Point>,
    state: menu::State,
    hovered: Option<usize>,
}

#[derive(Default)]
enum DragNodeState {
    #[default]
    Idle,
    Dragging {
        cursor_origin: Point,
        node_origin: HashMap<GraphNodeId, Point>,
    },
}

#[derive(Default)]
enum EdgeConnectState {
    #[default]
    Idle,
    Dragging {
        resolved_source: GraphSlotId,
        color: Color,
    },
}

#[derive(Default)]
struct NodeSelectionState {
    selected_nodes: HashSet<GraphNodeId>,
    state: DragSelectionState,
}

#[derive(Default)]
enum DragSelectionState {
    #[default]
    Idle,
    Dragging {
        cursor_origin: Point,
    },
}

#[derive(Default)]
enum ViewDragState {
    #[default]
    Idle,
    Dragging {
        cursor_origin: Point,
        translation_origin: Vector,
    },
}
