use std::collections::{HashMap, HashSet};

use cyancia_math::point::PointExt;
use gpui::{
    Action, Bounds, Context, InteractiveElement, IntoElement, MouseButton, MouseDownEvent,
    MouseMoveEvent, MouseUpEvent, ParentElement, PathBuilder, Pixels, Point, Render, SharedString,
    Size, Styled, Window, canvas, div, prelude::FluentBuilder, px,
};
use gpui_component::{ActiveTheme, ElementExt, menu::ContextMenuExt};
use schemars::JsonSchema;
use serde::Deserialize;

use crate::graph::{
    Graph, GraphData,
    node::{GraphNode, GraphNodeId, GraphNodeRegistry},
    slot::{GraphInputSlotId, GraphOutputSlotId},
};

#[derive(Action, Clone, PartialEq, Eq, Deserialize, JsonSchema)]
pub struct AddNodeAction {
    pub name: SharedString,
}

pub const SLOT_HIT_TEST_RADIUS_SQUARED: f64 = 10.0 * 10.0;

pub struct GraphEditor<Data: GraphData> {
    graph: Graph<Data>,
    node_registry: GraphNodeRegistry<Data>,

    node_drag_state: Option<DragState>,
    marquee_state: Option<MarqueeState>,
    slot_connect_state: Option<SlotConnectState>,
    pan_state: Option<PanState>,

    transform: ViewTransform,
    selected_nodes: HashSet<GraphNodeId>,
    node_bounds: HashMap<GraphNodeId, Bounds<Pixels>>,
    input_slot_pos: HashMap<GraphInputSlotId, Point<Pixels>>,
    output_slot_pos: HashMap<GraphOutputSlotId, Point<Pixels>>,
}

impl<Data: GraphData> GraphEditor<Data> {
    pub fn new(graph: Graph<Data>, node_registry: GraphNodeRegistry<Data>) -> Self {
        Self {
            graph,
            node_registry,

            node_drag_state: None,
            marquee_state: None,
            slot_connect_state: None,
            pan_state: None,

            transform: ViewTransform::default(),
            selected_nodes: HashSet::new(),
            node_bounds: HashMap::new(),
            input_slot_pos: HashMap::new(),
            output_slot_pos: HashMap::new(),
        }
    }

    pub fn on_add_node_action(
        &mut self,
        event: &AddNodeAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(node) = self.node_registry.get(&event.name) else {
            log::error!("Node type '{}' not found in registry", event.name);
            return;
        };

        let cursor = window.mouse_position();
        let pos = Point::new(cursor.x.into(), cursor.y.into());
        self.graph.add_boxed_node(pos, node);
    }

    pub fn on_left_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.slot_connect_start(event, window, cx);
        if self.slot_connect_state.is_some() {
            return;
        }

        self.node_drag_start(event, window, cx);
        if self.node_drag_state.is_some() {
            return;
        }

        self.marquee_start(event, window, cx);
        if self.marquee_state.is_some() {
            return;
        }

        cx.notify();
    }

    pub fn on_middle_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.pan_start(event, window, cx);
        if self.pan_state.is_some() {
            return;
        }
    }

    pub fn on_middle_mouse_up(
        &mut self,
        event: &MouseUpEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.pan_state.is_some() {
            self.pan_end(event, window, cx);
        }
    }

    pub fn on_mouse_move(
        &mut self,
        event: &MouseMoveEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.node_drag_state.is_some() {
            self.node_drag(event, window, cx);
        } else if self.slot_connect_state.is_some() {
            self.slot_connect_drag(event, window, cx);
        } else if self.marquee_state.is_some() {
            self.marquee_drag(event, window, cx);
        } else if self.pan_state.is_some() {
            self.pan_drag(event, window, cx);
        }
    }

    pub fn on_left_mouse_up(
        &mut self,
        event: &MouseUpEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.node_drag_state.is_some() {
            self.node_drag_end(event, window, cx);
        } else if self.slot_connect_state.is_some() {
            self.slot_connect_end(event, window, cx);
        } else if self.marquee_state.is_some() {
            self.marquee_end(event, window, cx);
        }
    }

    pub fn node_drag_start(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let mut node_id = None;
        for id in self.graph.nodes.keys() {
            let Some(bounds) = self.node_bounds.get(id) else {
                continue;
            };
            if bounds.contains(&event.position) {
                node_id = Some(*id);
                break;
            }
        }
        let Some(node_id) = node_id else {
            return;
        };
        dbg!(node_id);

        if self.selected_nodes.is_empty() {
            self.add_node_selection(node_id);
        } else {
            if event.modifiers.shift {
                self.add_node_selection(node_id);
            } else if event.modifiers.control {
                self.toggle_node_selection(node_id);
            } else if !self.selected_nodes.contains(&node_id) {
                self.selected_nodes.clear();
                self.add_node_selection(node_id);
            }
        }

        self.node_drag_state = Some(DragState {
            cursor_origin: window.mouse_position(),
            node_origins: self
                .selected_nodes
                .iter()
                .filter_map(|id| Some((id.clone(), self.graph.get_node(id)?.position)))
                .collect(),
        });
    }

    pub fn node_drag(
        &mut self,
        event: &MouseMoveEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(drag) = &mut self.node_drag_state else {
            return;
        };

        let offset = window.mouse_position() - drag.cursor_origin;
        let node_offset = Point::new(offset.x.into(), offset.y.into());

        for (id, origin) in &drag.node_origins {
            let pos = *origin + node_offset;
            if let Some(node) = self.graph.get_node_mut(id) {
                node.position = pos;
            }
        }

        cx.notify();
    }

    pub fn node_drag_end(
        &mut self,
        event: &MouseUpEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.node_drag_state = None;
        cx.notify();
    }

    pub fn marquee_start(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let marquee_mode = if event.modifiers.shift {
            MarqueeMode::Add
        } else {
            MarqueeMode::Replace
        };

        if marquee_mode == MarqueeMode::Replace {
            self.selected_nodes.clear();
        }

        self.marquee_state = Some(MarqueeState {
            cursor_origin: window.mouse_position(),
            mode: marquee_mode,
            originally_selected: self.selected_nodes.clone(),
        });
    }

    pub fn marquee_drag(
        &mut self,
        event: &MouseMoveEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(marquee) = &mut self.marquee_state else {
            return;
        };

        let marquee_bounds = marquee.bounds(window.mouse_position());
        let mut selected_nodes = match marquee.mode {
            MarqueeMode::Replace => HashSet::new(),
            MarqueeMode::Add => marquee.originally_selected.clone(),
        };

        for (id, bounds) in &self.node_bounds {
            if !marquee_bounds.intersects(bounds) {
                continue;
            }

            selected_nodes.insert(*id);
        }

        self.selected_nodes = selected_nodes;

        cx.notify();
    }

    pub fn marquee_end(
        &mut self,
        event: &MouseUpEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.marquee_state = None;
        cx.notify();
    }

    pub fn slot_connect_start(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let mut start_slot = None;
        for (id, pos) in &self.input_slot_pos {
            if event.position.relative_to(&pos).magnitude_squared() <= SLOT_HIT_TEST_RADIUS_SQUARED
            {
                start_slot = Some(GraphSlotId::Input(*id));
                break;
            }
        }
        for (id, pos) in &self.output_slot_pos {
            if event.position.relative_to(&pos).magnitude_squared() <= SLOT_HIT_TEST_RADIUS_SQUARED
            {
                start_slot = Some(GraphSlotId::Output(*id));
                break;
            }
        }

        let Some(mut start_slot) = start_slot else {
            return;
        };
        dbg!(start_slot);

        match start_slot {
            GraphSlotId::Input(slot_id) => {
                let Some(slot) = self.graph.slots.inputs.get_mut(&slot_id) else {
                    return;
                };

                if let Some(connected) = slot.connected.take() {
                    start_slot = GraphSlotId::Output(connected);
                }
            }
            GraphSlotId::Output(slot_id) => {}
        }

        self.slot_connect_state = Some(SlotConnectState { start_slot });

        cx.notify();
    }

    pub fn slot_connect_drag(
        &mut self,
        event: &MouseMoveEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(slot_connect_state) = &self.slot_connect_state else {
            return;
        };

        cx.notify();
    }

    pub fn slot_connect_end(
        &mut self,
        event: &MouseUpEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(slot_connect_state) = self.slot_connect_state.take() else {
            return;
        };

        let mut end_slot = None;
        for (slot_id, pos) in &self.input_slot_pos {
            if event.position.relative_to(&pos).magnitude_squared() <= SLOT_HIT_TEST_RADIUS_SQUARED
            {
                end_slot = Some(GraphSlotId::Input(*slot_id));
                break;
            }
        }
        for (slot_id, pos) in &self.output_slot_pos {
            if event.position.relative_to(&pos).magnitude_squared() <= SLOT_HIT_TEST_RADIUS_SQUARED
            {
                end_slot = Some(GraphSlotId::Output(*slot_id));
                break;
            }
        }

        let Some(end_slot) = end_slot else {
            cx.notify();
            return;
        };
        dbg!(end_slot);

        match (slot_connect_state.start_slot, end_slot) {
            (GraphSlotId::Input(to), GraphSlotId::Output(from)) => {
                self.graph.connect_slots(from, to);
            }
            (GraphSlotId::Output(from), GraphSlotId::Input(to)) => {
                self.graph.connect_slots(from, to);
            }
            _ => return,
        }

        cx.notify();
    }

    pub fn pan_start(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.pan_state = Some(PanState {
            cursor_origin: event.position,
            original_translation: self.transform.translation,
        });
    }

    pub fn pan_drag(
        &mut self,
        event: &MouseMoveEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(pan_state) = &mut self.pan_state else {
            return;
        };

        let offset = event.position - pan_state.cursor_origin;
        self.transform.translation =
            pan_state.original_translation + Point::new(offset.x.into(), offset.y.into());
        cx.notify();
    }

    pub fn pan_end(&mut self, event: &MouseUpEvent, window: &mut Window, cx: &mut Context<Self>) {
        self.pan_state = None;
    }

    pub fn get_node_state_mut<T: GraphNode<Data>>(
        &mut self,
        id: &GraphNodeId,
    ) -> Option<&mut T::State> {
        dbg!(std::any::type_name::<T>());
        self.graph.nodes.get_mut(id)?.data.state_mut::<T>()
    }

    pub fn add_node_selection(&mut self, id: GraphNodeId) {
        self.selected_nodes.insert(id);
    }

    pub fn toggle_node_selection(&mut self, id: GraphNodeId) {
        if self.selected_nodes.contains(&id) {
            self.selected_nodes.remove(&id);
        } else {
            self.selected_nodes.insert(id);
        }
    }

    pub fn add_input_slot_pos(&mut self, id: GraphInputSlotId, pos: Point<Pixels>) {
        self.input_slot_pos.insert(id, pos);
    }

    pub fn add_output_slot_pos(&mut self, id: GraphOutputSlotId, pos: Point<Pixels>) {
        self.output_slot_pos.insert(id, pos);
    }
}

impl<Data: GraphData> Render for GraphEditor<Data> {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let mut nodes = Vec::with_capacity(self.graph.nodes.len());
        let mut node_ids = Vec::with_capacity(self.graph.nodes.len());

        for (id, node) in &self.graph.nodes {
            let body = node.render(
                *id,
                &self.graph.slots,
                &self.graph.resources,
                &self.graph.type_registry,
                window,
                cx,
            );
            let position = node.position + self.transform.translation;

            let node = div()
                .w(px(170.0))
                .id(**id)
                .absolute()
                .bg(cx.theme().background)
                .left(px(position.x))
                .top(px(position.y))
                .border_2()
                .when(self.selected_nodes.contains(id), |div| {
                    div.border_color(cx.theme().foreground)
                })
                .child(div().bg(node.data.header_color()).child(node.data.name()))
                .child(body)
                .into_any_element();

            nodes.push(node);
            node_ids.push(*id);
        }

        let all_nodes = self.node_registry.all().keys().cloned().collect::<Vec<_>>();
        div()
            .w_full()
            .h_full()
            .when_some(self.marquee_state.as_ref(), |d, marquee| {
                let marquee_bounds = marquee.bounds(window.mouse_position());
                d.child(
                    div()
                        .absolute()
                        .left(marquee_bounds.origin.x)
                        .top(marquee_bounds.origin.y)
                        .w(marquee_bounds.size.width)
                        .h(marquee_bounds.size.height)
                        .bg(cx.theme().accent)
                        .border_2()
                        .border_color(cx.theme().border),
                )
            })
            .child(
                div()
                    .absolute()
                    .size_full()
                    .children(nodes)
                    .on_children_prepainted({
                        let editor = cx.entity().downgrade();
                        move |bounds, _window, cx| {
                            editor.update(cx, |editor, _cx| {
                                editor.node_bounds.clear();
                                for (node_id, bounds) in node_ids.iter().zip(bounds) {
                                    editor.node_bounds.insert(*node_id, bounds);
                                }
                            });
                        }
                    }),
            )
            .child(canvas(|_, _, _| {}, {
                let connecting =
                    self.slot_connect_state
                        .iter()
                        .filter_map(|st| match st.start_slot {
                            GraphSlotId::Input(input_id) => {
                                let pos = self.input_slot_pos.get(&input_id)?;
                                let slot = self.graph.slots.inputs.get(&input_id)?;
                                Some((*pos, window.mouse_position(), slot.data.ty().color()))
                            }
                            GraphSlotId::Output(output_id) => {
                                let pos = self.output_slot_pos.get(&output_id)?;
                                let slot = self.graph.slots.outputs.get(&output_id)?;
                                Some((*pos, window.mouse_position(), slot.data_ty.color()))
                            }
                        });
                let segments = self
                    .graph
                    .slots
                    .inputs
                    .iter()
                    .filter_map(|(input_id, input)| {
                        let output_id = &input.connected?;
                        let from = self.input_slot_pos.get(input_id)?;
                        let to = self.output_slot_pos.get(output_id)?;
                        Some((*from, *to, input.data.ty().color()))
                    })
                    .chain(connecting)
                    .collect::<Vec<_>>();

                self.input_slot_pos.clear();
                self.output_slot_pos.clear();

                move |bounds, _, window, cx| {
                    for (from, to, color) in segments {
                        let mut builder = PathBuilder::stroke(px(2.0));

                        builder.move_to(from);
                        builder.line_to(to);

                        if let Ok(path) = builder.build() {
                            window.paint_path(path, color);
                        }
                    }
                }
            }))
            .on_action(cx.listener(Self::on_add_node_action))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::on_left_mouse_down))
            .on_mouse_down(MouseButton::Middle, cx.listener(Self::on_middle_mouse_down))
            .on_mouse_move(cx.listener(Self::on_mouse_move))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::on_left_mouse_up))
            .on_mouse_up(MouseButton::Middle, cx.listener(Self::on_middle_mouse_up))
            .context_menu(move |menu, window, cx| {
                let all_nodes = all_nodes.clone();
                menu.submenu("Add Node", window, cx, move |mut menu, window, cx| {
                    for node in all_nodes.iter() {
                        menu = menu.menu(
                            *node,
                            Box::new(AddNodeAction {
                                name: (*node).into(),
                            }),
                        )
                    }
                    menu
                })
            })
    }
}

struct DragState {
    cursor_origin: Point<Pixels>,
    node_origins: HashMap<GraphNodeId, Point<f32>>,
}

struct MarqueeState {
    cursor_origin: Point<Pixels>,
    originally_selected: HashSet<GraphNodeId>,
    mode: MarqueeMode,
}

impl MarqueeState {
    fn bounds(&self, cursor_current: Point<Pixels>) -> Bounds<Pixels> {
        let min = cursor_current.min(&self.cursor_origin);
        let max = cursor_current.max(&self.cursor_origin);
        Bounds::new(min, Size::new(max.x - min.x, max.y - min.y))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MarqueeMode {
    Replace,
    Add,
}

#[derive(Debug, Clone, Copy)]
struct SlotConnectState {
    start_slot: GraphSlotId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum GraphSlotId {
    Input(GraphInputSlotId),
    Output(GraphOutputSlotId),
}

struct PanState {
    cursor_origin: Point<Pixels>,
    original_translation: Point<f32>,
}

#[derive(Default)]
struct ViewTransform {
    translation: Point<f32>,
}
