use std::{collections::HashMap, ops::Range};

use cyancia_widgets::drag_field::DragField;
use iced_core::{
    Background, Border, Color, Element, Event, Layout, Length, Point, Shadow, Size, Vector,
    alignment::Horizontal,
    border::{self, Radius},
    gradient::ColorStop,
    layout::{self, Limits, Node},
    mouse::{self, Interaction},
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
    row, text,
};

use crate::{
    ErasedSlotValue, Graph, GraphInputSlot, GraphNodeData, GraphOutputSlot, GraphSlotValueType,
    GraphSlots, InputSlotId, NodeId, OutputSlotId,
    editor::helpers::{SlotSide, empty_slot, valued_slot},
};

pub mod drawer;
pub mod helpers;

#[derive(Debug, Clone, Copy)]
pub struct Style {
    pub node_background: Background,
    pub node_shadow: Option<Shadow>,
}

pub trait Catalog {
    type Class<'a>;

    fn default<'a>() -> Self::Class<'a>;

    fn style(&self, class: &Self::Class<'_>) -> Style;
}

/// A styling function for a [`Container`].
pub type StyleFn<'a, Theme> = Box<dyn Fn(&Theme) -> Style + 'a>;

impl<Theme> From<Style> for StyleFn<'_, Theme> {
    fn from(style: Style) -> Self {
        Box::new(move |_theme| style)
    }
}

impl Catalog for iced_core::Theme {
    type Class<'a> = StyleFn<'a, Self>;

    fn default<'a>() -> Self::Class<'a> {
        Box::new(|t| {
            let palette = t.extended_palette();
            Style {
                node_background: palette.background.strong.color.into(),
                node_shadow: Some(Shadow {
                    color: Color::BLACK,
                    offset: Vector::new(2.0, 2.0),
                    blur_radius: 5.0,
                    ..Default::default()
                }),
            }
        })
    }

    fn style(&self, class: &Self::Class<'_>) -> Style {
        class(self)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GraphSlotId {
    Input(InputSlotId),
    Output(OutputSlotId),
}

#[derive(Debug)]
pub enum GraphEditorMessage<Message> {
    NodeMoved(Point, NodeId),
    EdgeCreated(OutputSlotId, InputSlotId),
    EdgeRemoved(InputSlotId),
    Custom(Message),
}

pub trait GraphSlotViewer<'a, Message, Theme, Renderer>: GraphSlotValueType {
    fn view(
        &self,
        name: &'static str,
        value: &ErasedSlotValue,
        slot_id: GraphSlotId,
    ) -> Option<Element<'a, Message, Theme, Renderer>>;
}

pub struct GraphSlotViewers<'a, Message, Theme, Renderer> {
    viewers: HashMap<&'static str, Box<dyn GraphSlotViewer<'a, Message, Theme, Renderer>>>,
}

impl<'a, Message, Theme, Renderer> GraphSlotViewers<'a, Message, Theme, Renderer>
where
    Message: 'a,
    Theme: text::Catalog + 'a,
    Renderer: iced_core::Renderer + iced_core::text::Renderer + 'a,
{
    pub fn new() -> Self {
        Self {
            viewers: HashMap::new(),
        }
    }

    pub fn register<V: GraphSlotViewer<'a, Message, Theme, Renderer> + 'static>(
        &mut self,
        viewer: V,
    ) {
        self.viewers.insert(viewer.type_name(), Box::new(viewer));
    }

    pub fn view_input(
        &self,
        id: InputSlotId,
        slot: &GraphInputSlot,
    ) -> Option<Element<'a, Message, Theme, Renderer>> {
        let viewer = self.viewers.get(slot.value_type.type_name())?;
        if slot.connected.is_some() {
            Some(empty_slot(viewer.color(), slot.name, SlotSide::Left))
        } else {
            Some(
                viewer
                    .view(slot.name, &slot.value, GraphSlotId::Input(id))
                    .map(|widget| valued_slot(viewer.color(), slot.name, SlotSide::Left, widget))
                    .unwrap_or_else(|| empty_slot(viewer.color(), slot.name, SlotSide::Left)),
            )
        }
    }

    pub fn view_output(
        &self,
        id: OutputSlotId,
        slot: &GraphOutputSlot,
    ) -> Option<Element<'a, Message, Theme, Renderer>> {
        let viewer = self.viewers.get(slot.value_type.type_name())?;
        Some(empty_slot(viewer.color(), slot.name, SlotSide::Right))
    }
}

pub struct GraphView<'a, Message, Theme, Renderer>
where
    Message: 'a,
    Theme: container::Catalog + text::Catalog + Catalog + 'a,
    <Theme as container::Catalog>::Class<'a>: From<container::StyleFn<'a, Theme>>,
    Renderer: iced_core::Renderer + iced_core::text::Renderer + 'a,
{
    graph: DrawableGraph<'a, Message, Theme, Renderer>,
}

impl<'a, Message, Theme, Renderer: iced_core::Renderer> GraphView<'a, Message, Theme, Renderer>
where
    Message: 'a,
    Theme: container::Catalog + text::Catalog + Catalog + 'a,
    <Theme as container::Catalog>::Class<'a>: From<container::StyleFn<'a, Theme>>,
    Renderer: iced_core::Renderer + iced_core::text::Renderer + 'a,
{
    pub fn new(graph: &Graph, viewers: &GraphSlotViewers<'a, Message, Theme, Renderer>) -> Self {
        Self {
            graph: DrawableGraph::new(graph, viewers),
        }
    }
}

pub struct GraphNodeStyle {
    pub background: Background,
    pub padding: f32,
    pub line_height: f32,
    pub line_spacing: f32,
}

pub struct DrawableGraph<'a, Message, Theme, Renderer>
where
    Theme: Catalog,
{
    pub nodes: Vec<DrawableNode<'a, Message, Theme, Renderer>>,
    pub slots_positions: HashMap<GraphSlotId, Point>,
    pub slots: HashMap<GraphSlotId, SlotData>,
    pub edges: HashMap<InputSlotId, DrawableEdge>,
}

impl<'a, Message, Theme, Renderer: iced_core::Renderer> DrawableGraph<'a, Message, Theme, Renderer>
where
    Message: 'a,
    Theme: container::Catalog + text::Catalog + Catalog + 'a,
    <Theme as container::Catalog>::Class<'a>: From<container::StyleFn<'a, Theme>>,
    Renderer: iced_core::Renderer + iced_core::text::Renderer + 'a,
{
    pub fn new(graph: &Graph, viewers: &GraphSlotViewers<'a, Message, Theme, Renderer>) -> Self {
        let style = <Theme as Catalog>::default();
        let mut nodes = Vec::with_capacity(graph.nodes.len());
        let mut node_indices = HashMap::with_capacity(graph.nodes.len());
        for (index, (id, node)) in graph.nodes.iter().enumerate() {
            nodes.push(DrawableNode::new(*id, node, &graph.slots, &viewers));
            node_indices.insert(*id, index);
        }

        let edges = graph
            .slots
            .inputs
            .iter()
            .filter_map(|(to, to_slot)| {
                let from = graph.slots.inputs.get(&to)?.connected?;
                let from_slot = graph.slots.outputs.get(&from)?;

                let from_color = from_slot.value_type.color();
                let to_color = to_slot.value_type.color();
                let style = if from_color == to_color {
                    geometry::Style::Solid(from_color)
                } else {
                    // let g = Linear::new(Point::new(0.0, 0.0), Point::new(1.0, 1.0)).add_stops([
                    //     ColorStop {
                    //         offset: 0.0,
                    //         color: from_color,
                    //     },
                    //     ColorStop {
                    //         offset: 1.0,
                    //         color: to_color,
                    //     },
                    // ]);
                    // geometry::Style::Gradient(g.into())
                    geometry::Style::Solid(from_color)
                };

                Some((*to, DrawableEdge { from, style }))
            })
            .collect();

        let slots = graph
            .slots
            .inputs
            .iter()
            .filter_map(|(id, slot)| {
                viewers
                    .viewers
                    .get(slot.value_type.type_name())
                    .map(|v| v.color())
                    .map(|color| (GraphSlotId::Input(*id), SlotData { color }))
            })
            .chain(graph.slots.outputs.iter().filter_map(|(id, slot)| {
                viewers
                    .viewers
                    .get(slot.value_type.type_name())
                    .map(|v| v.color())
                    .map(|color| (GraphSlotId::Output(*id), SlotData { color }))
            }))
            .collect();

        Self {
            nodes,
            slots_positions: HashMap::default(),
            edges,
            slots,
        }
    }
}

pub struct SlotData {
    pub color: Color,
}

pub struct DrawableEdge {
    from: OutputSlotId,
    style: geometry::Style,
}

pub struct DrawableNode<'a, Message, Theme, Renderer> {
    pub node_id: NodeId,
    pub position: Point,
    pub widget: Element<'a, GraphEditorMessage<Message>, Theme, Renderer>,
    pub input_slots: Vec<InputSlotId>,
    pub output_slots: Vec<OutputSlotId>,
}

impl<'a, Message, Theme, Renderer> DrawableNode<'a, Message, Theme, Renderer>
where
    Message: 'a,
    Theme: container::Catalog + text::Catalog + Catalog + 'a,
    <Theme as container::Catalog>::Class<'a>: From<container::StyleFn<'a, Theme>>,
    Renderer: iced_core::Renderer + iced_core::text::Renderer + 'a,
{
    pub fn new(
        node_id: NodeId,
        node: &GraphNodeData,
        slots: &GraphSlots,
        viewers: &GraphSlotViewers<'a, Message, Theme, Renderer>,
    ) -> Self {
        const NODE_WIDTH: f32 = 170.0;
        const NODE_BORDER_RADIUS: f32 = 5.0;

        let inputs = node
            .inputs
            .iter()
            .filter_map(|slot_id| slots.inputs.get(slot_id).map(|slot| (slot_id, slot)))
            .filter_map(|(slot_id, slot)| viewers.view_input(*slot_id, slot))
            .map(|e| e.map(GraphEditorMessage::Custom));
        let outputs = node
            .outputs
            .iter()
            .filter_map(|slot_id| slots.outputs.get(slot_id).map(|slot| (slot_id, slot)))
            .filter_map(|(slot_id, slot)| viewers.view_output(*slot_id, slot))
            .map(|e| e.map(GraphEditorMessage::Custom));
        let inputs = column(inputs).spacing(2);
        let outputs = column(outputs).spacing(2);
        let header_color = node.node.header_color();
        let header = container(column![text(node.node.name()), text(node_id.to_string()),])
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
            .align_x(Horizontal::Center)
            .padding(5);

        // .on_drag(move |btn, point| {
        //     if btn == mouse::Button::Left
        //         && let Some(p) = point
        //     {
        //         Some(GraphEditorMessage::NodeMoved(p, node_id))
        //     } else {
        //         None
        //     }
        // });

        let widget = container(
            column![
                header,
                column![
                    inputs.width(NODE_WIDTH - 50.0),
                    row![row![].width(Length::Fill), outputs]
                ]
                .padding(5)
            ]
            .width(NODE_WIDTH),
        )
        .style(move |t: &Theme| {
            let class = <Theme as Catalog>::default();
            let style = <Theme as Catalog>::style(t, &class);
            container::Style {
                background: Some(style.node_background),
                shadow: style.node_shadow.unwrap_or_default(),
                border: Border::default().rounded(NODE_BORDER_RADIUS),
                ..Default::default()
            }
        });

        Self {
            node_id,
            position: node.position,
            widget: Element::new(widget),
            input_slots: node.inputs.clone(),
            output_slots: node.outputs.clone(),
        }
    }

    pub fn update_slot_positions(
        &mut self,
        node: &Layout<'_>,
        slot_positions: &mut HashMap<GraphSlotId, Point>,
    ) {
        let slots = &node.child(0).child(1);
        let inputs = slots.child(0).children();
        let outputs = slots.child(1).child(1).children();

        for (slot_id, layout) in self.input_slots.iter().zip(inputs) {
            let pin = &layout.children().next().unwrap();
            slot_positions.insert(
                GraphSlotId::Input(*slot_id),
                Point::new(pin.bounds().center_x(), pin.bounds().center_y()),
            );
        }
        for (slot_id, layout) in self.output_slots.iter().zip(outputs) {
            let pin = &layout.children().rev().next().unwrap();
            slot_positions.insert(
                GraphSlotId::Output(*slot_id),
                Point::new(pin.bounds().center_x(), pin.bounds().center_y()),
            );
        }
    }
}

impl<'a, Message, Renderer: iced_core::Renderer>
    Widget<GraphEditorMessage<Message>, iced_core::Theme, Renderer>
    for GraphView<'a, Message, iced_core::Theme, Renderer>
where
    Message: 'a,
    // Theme: container::Catalog + text::Catalog + Catalog + 'a,
    // <Theme as container::Catalog>::Class<'a>: From<container::StyleFn<'a, Theme>>,
    Renderer:
        iced_core::Renderer + iced_core::text::Renderer + iced_graphics::geometry::Renderer + 'a,
{
    fn children(&self) -> Vec<Tree> {
        self.graph
            .nodes
            .iter()
            .map(|node| Tree::new(&node.widget))
            .collect()
    }

    fn diff(&self, tree: &mut Tree) {
        tree.diff_children(
            &self
                .graph
                .nodes
                .iter()
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

    fn layout(&mut self, tree: &mut Tree, renderer: &Renderer, limits: &layout::Limits) -> Node {
        let children = self
            .graph
            .nodes
            .iter_mut()
            .zip(&mut tree.children)
            .map(|(node, tree)| {
                node.widget
                    .as_widget_mut()
                    .layout(tree, renderer, &Limits::NONE)
                    .translate(Vector::new(node.position.x, node.position.y))
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
        renderer: &Renderer,
        operation: &mut dyn Operation,
    ) {
        operation.container(None, layout.bounds());
        operation.traverse(&mut |operation| {
            self.graph
                .nodes
                .iter_mut()
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
        cursor: iced_core::mouse::Cursor,
        renderer: &Renderer,
        clipboard: &mut dyn iced_core::Clipboard,
        shell: &mut iced_core::Shell<'_, GraphEditorMessage<Message>>,
        viewport: &Rectangle,
    ) {
        for ((child, tree), layout) in self
            .graph
            .nodes
            .iter_mut()
            .zip(&mut tree.children)
            .zip(layout.children())
        {
            child.widget.as_widget_mut().update(
                tree, event, layout, cursor, renderer, clipboard, shell, viewport,
            );

            child.update_slot_positions(&layout, &mut self.graph.slots_positions);
        }

        if shell.is_event_captured() {
            return;
        }

        let state = tree.state.downcast_mut::<State>();

        const SLOT_PIN_SNAP: f32 = 3.0 * 3.0;
        match event {
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                let Some(cursor) = cursor.position() else {
                    return;
                };

                for (slot_id, slot_pos) in &self.graph.slots_positions {
                    let d = slot_pos.distance(cursor);
                    if d < SLOT_PIN_SNAP {
                        let resolved_source = match slot_id {
                            GraphSlotId::Input(id) => {
                                shell.publish(GraphEditorMessage::EdgeRemoved(*id));

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

                        state.edge_connect = EdgeConnectState::Dragging {
                            resolved_source,
                            color: slot_data.color,
                        };
                        shell.capture_event();
                        return;
                    }
                }

                for (node_index, layout) in layout.children().enumerate() {
                    if layout.bounds().contains(cursor) {
                        state.drag = DragState::Grabbed {
                            node_index,
                            origin: cursor,
                        };
                        shell.capture_event();
                        return;
                    }
                }
            }
            Event::Mouse(mouse::Event::CursorMoved { .. }) => {
                match &state.edge_connect {
                    EdgeConnectState::Idle => {}
                    EdgeConnectState::Dragging { .. } => {
                        shell.request_redraw();
                        return;
                    }
                }

                let Some(cursor) = cursor.position() else {
                    return;
                };

                match state.drag {
                    DragState::Idle => {}
                    DragState::Grabbed { node_index, origin } => {
                        if origin.distance(cursor) > 5.0 {
                            state.drag = DragState::Dragging {
                                node_index: node_index,
                                offset: origin - layout.child(node_index).bounds().position(),
                            };
                            shell.capture_event();
                        }
                    }
                    DragState::Dragging { node_index, offset } => {
                        let node_id = self.graph.nodes[node_index].node_id;
                        let relative = cursor - layout.bounds().position();
                        shell.publish(GraphEditorMessage::NodeMoved(
                            Point::new(relative.x, relative.y) - offset,
                            node_id,
                        ));
                    }
                }
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                if let EdgeConnectState::Dragging {
                    resolved_source,
                    color,
                } = &state.edge_connect
                {
                    let mut found = None;
                    for (slot_id, slot_pos) in &self.graph.slots_positions {
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
                                shell.publish(GraphEditorMessage::EdgeCreated(from, to));
                            }
                            (GraphSlotId::Output(from), GraphSlotId::Input(to)) => {
                                shell.publish(GraphEditorMessage::EdgeCreated(from, to));
                            }
                            _ => {}
                        }
                    }
                    state.edge_connect = EdgeConnectState::Idle;
                    shell.capture_event();
                    shell.request_redraw();
                    return;
                }

                if let DragState::Dragging { .. } = state.drag {
                    state.drag = DragState::Idle;
                    shell.capture_event();
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
        renderer: &Renderer,
    ) -> Interaction {
        let state = tree.state.downcast_ref::<State>();

        match state.drag {
            DragState::Idle => self
                .graph
                .nodes
                .iter()
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
            DragState::Grabbed { .. } | DragState::Dragging { .. } => {
                return mouse::Interaction::Grabbing;
            }
        }
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut Renderer,
        theme: &iced_core::Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: Cursor,
        viewport: &Rectangle,
    ) {
        let state = tree.state.downcast_ref::<State>();

        renderer.fill_quad(
            Quad {
                bounds: layout.bounds(),
                ..Default::default()
            },
            theme.extended_palette().background.base.color,
        );

        for ((child, tree), layout) in self
            .graph
            .nodes
            .iter()
            .zip(&tree.children)
            .zip(layout.children())
            .filter(|(_, layout)| layout.bounds().intersects(viewport))
        {
            child
                .widget
                .as_widget()
                .draw(tree, renderer, theme, style, layout, cursor, viewport);
        }

        let mut frame = Frame::new(renderer, layout.bounds().size());
        for (to, edge) in &self.graph.edges {
            let from_pos = self
                .graph
                .slots_positions
                .get(&GraphSlotId::Output(edge.from));
            let to_pos = self.graph.slots_positions.get(&GraphSlotId::Input(*to));
            if let (Some(from_pos), Some(to_pos)) = (from_pos, to_pos) {
                frame.stroke(
                    &geometry::Path::line(*from_pos, *to_pos),
                    Stroke {
                        style: edge.style,
                        width: 2.0,
                        ..Default::default()
                    },
                );
            }
        }

        if let (
            EdgeConnectState::Dragging {
                resolved_source,
                color,
            },
            Some(cursor_pos),
        ) = (&state.edge_connect, cursor.position())
            && let Some(start_pos) = self.graph.slots_positions.get(&resolved_source)
        {
            frame.stroke(
                &geometry::Path::line(*start_pos, cursor_pos),
                Stroke {
                    style: (*color).into(),
                    width: 2.0,
                    ..Default::default()
                },
            );
        };

        renderer.draw_geometry(frame.into_geometry());
    }
}

impl<'a, Message, Renderer>
    Into<Element<'a, GraphEditorMessage<Message>, iced_core::Theme, Renderer>>
    for GraphView<'a, Message, iced_core::Theme, Renderer>
where
    Message: 'a,
    // Theme: container::Catalog + text::Catalog + Catalog + 'a,
    // <Theme as container::Catalog>::Class<'a>: From<container::StyleFn<'a, Theme>>,
    Renderer:
        iced_core::Renderer + iced_core::text::Renderer + iced_graphics::geometry::Renderer + 'a,
{
    fn into(self) -> Element<'a, GraphEditorMessage<Message>, iced_core::Theme, Renderer> {
        Element::new(self)
    }
}

#[derive(Default)]
struct State {
    drag: DragState,
    edge_connect: EdgeConnectState,
}

#[derive(Default)]
enum DragState {
    #[default]
    Idle,
    Grabbed {
        node_index: usize,
        origin: Point,
    },
    Dragging {
        node_index: usize,
        offset: Vector,
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
