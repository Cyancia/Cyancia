use std::collections::{HashMap, HashSet};

use gpui::{
    Action, Bounds, Context, InteractiveElement, IntoElement, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, ParentElement, Pixels, Point, Render, SharedString, Styled, Window, div, prelude::FluentBuilder, px
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
    selected_nodes: HashSet<GraphNodeId>,
}

impl<Data: GraphData> GraphEditor<Data> {
    pub fn new(graph: Graph<Data>, node_registry: GraphNodeRegistry<Data>) -> Self {
        Self {
            graph,
            node_registry,
            drag_state: None,
            selected_nodes: HashSet::new(),
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

    pub fn on_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if event.button != MouseButton::Left {
            return;
        }

        self.selected_nodes.clear();
        cx.notify();
    }

    pub fn on_mouse_move(
        &mut self,
        event: &MouseMoveEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if event.pressed_button != Some(MouseButton::Left) {
            return;
        }

        match &self.drag_state {
            Some(drag) => {
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
            None => {
                self.drag_state = Some(DragState {
                    cursor_origin: window.mouse_position(),
                    node_origins: self
                        .selected_nodes
                        .iter()
                        .filter_map(|id| Some((id.clone(), self.graph.get_node(id)?.position)))
                        .collect(),
                });
            }
        }
    }

    pub fn on_mouse_up(
        &mut self,
        event: &MouseUpEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.drag_state = None;
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
        let nodes = self.graph.nodes.iter().map(|(id, node)| {
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
                                } else {
                                    editor.selected_nodes.clear();
                                    editor.add_node_selection(node_id);
                                }
                            }

                            cx.stop_propagation();
                        });
                    }
                })
        });

        let all_nodes = self.node_registry.all().keys().cloned().collect::<Vec<_>>();
        div()
            .w_full()
            .h_full()
            .children(nodes)
            .on_action(cx.listener(Self::on_add_node_action))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::on_mouse_down))
            .on_mouse_move(cx.listener(Self::on_mouse_move))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::on_mouse_up))
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
