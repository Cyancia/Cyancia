use std::collections::{HashMap, HashSet};

use gpui::{
    Action, Bounds, Context, InteractiveElement, IntoElement, MouseButton, MouseDownEvent,
    MouseMoveEvent, MouseUpEvent, ParentElement, Pixels, Point, Render, SharedString, Size, Styled,
    Window, div, prelude::FluentBuilder, px,
};
use gpui_component::{ActiveTheme, ElementExt, menu::ContextMenuExt};
use schemars::JsonSchema;
use serde::Deserialize;

use crate::graph::{
    Graph, GraphData,
    node::{GraphNode, GraphNodeId, GraphNodeRegistry},
};

#[derive(Action, Clone, PartialEq, Eq, Deserialize, JsonSchema)]
pub struct AddNodeAction {
    pub name: SharedString,
}

pub struct GraphEditor<Data: GraphData> {
    graph: Graph<Data>,
    node_registry: GraphNodeRegistry<Data>,
    drag_state: Option<DragState>,
    marquee_state: Option<MarqueeState>,
    selected_nodes: HashSet<GraphNodeId>,
    node_bounds: HashMap<GraphNodeId, Bounds<Pixels>>,
}

impl<Data: GraphData> GraphEditor<Data> {
    pub fn new(graph: Graph<Data>, node_registry: GraphNodeRegistry<Data>) -> Self {
        Self {
            graph,
            node_registry,
            drag_state: None,
            marquee_state: None,
            selected_nodes: HashSet::new(),
            node_bounds: HashMap::new(),
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
        self.drag_state = None;
        cx.notify();
    }

    pub fn on_mouse_move(
        &mut self,
        event: &MouseMoveEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.drag_state.is_some() {
            self.node_drag(event, window, cx);
        } else if self.marquee_state.is_some() {
            self.marquee_drag(event, window, cx);
        }
    }

    pub fn on_left_mouse_up(
        &mut self,
        event: &MouseUpEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.marquee_state.take().is_some() {
            cx.notify();
        }
    }

    pub fn node_drag_start(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if event.button != MouseButton::Left {
            return;
        }

        self.drag_state = Some(DragState {
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
        if event.pressed_button != Some(MouseButton::Left) {
            return;
        }

        let Some(drag) = &mut self.drag_state else {
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
}

impl<Data: GraphData> Render for GraphEditor<Data> {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.node_bounds.clear();

        let nodes = self
            .graph
            .nodes
            .iter()
            .map(|(id, node)| {
                let body = node.render(
                    *id,
                    &self.graph.slots,
                    &self.graph.resources,
                    &self.graph.type_registry,
                    window,
                    cx,
                );

                div()
                    .w(px(170.0))
                    .id(**id)
                    .absolute()
                    .left(Pixels::from(node.position.x))
                    .top(Pixels::from(node.position.y))
                    .border_2()
                    .when(self.selected_nodes.contains(id), |div| {
                        div.border_color(cx.theme().foreground)
                    })
                    .child(div().bg(node.data.header_color()).child(node.data.name()))
                    .child(body)
                    .on_mouse_down(MouseButton::Left, {
                        let node_id = *id;
                        let editor = cx.entity().downgrade();
                        move |event, window, cx| {
                            editor.update(cx, |editor, cx| {
                                if editor.selected_nodes.is_empty() {
                                    editor.add_node_selection(node_id);
                                } else {
                                    if event.modifiers.shift {
                                        editor.add_node_selection(node_id);
                                    } else if event.modifiers.control {
                                        editor.toggle_node_selection(node_id);
                                    } else if !editor.selected_nodes.contains(&node_id) {
                                        editor.selected_nodes.clear();
                                        editor.add_node_selection(node_id);
                                    }
                                }

                                editor.node_drag_start(event, window, cx);

                                cx.stop_propagation();
                            });
                        }
                    })
                    .on_prepaint({
                        let node_id = *id;
                        let editor = cx.entity().downgrade();
                        move |bounds, window, cx| {
                            editor.update(cx, |editor, cx| {
                                editor.node_bounds.insert(node_id, bounds);
                            });
                        }
                    })
            })
            .collect::<Vec<_>>();

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
            .children(nodes)
            .on_action(cx.listener(Self::on_add_node_action))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::on_left_mouse_down))
            .on_mouse_move(cx.listener(Self::on_mouse_move))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::on_left_mouse_up))
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
