use gpui::{
    Action, Context, InteractiveElement, IntoElement, ParentElement, Pixels, Point, Render,
    SharedString, Styled, Window, div, px,
};
use gpui_component::menu::ContextMenuExt;
use schemars::JsonSchema;
use serde::Deserialize;

use crate::graph::{Graph, GraphData, node::GraphNodeRegistry};

#[derive(Action, Clone, PartialEq, Eq, Deserialize, JsonSchema)]
pub struct AddNodeAction {
    pub name: SharedString,
}

pub struct GraphEditor<Data: GraphData> {
    graph: Graph<Data>,
    node_registry: GraphNodeRegistry<Data>,
}

impl<Data: GraphData> GraphEditor<Data> {
    pub fn new(graph: Graph<Data>, node_registry: GraphNodeRegistry<Data>) -> Self {
        Self {
            graph,
            node_registry,
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
}

impl<Data: GraphData> Render for GraphEditor<Data> {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let nodes = self.graph.nodes.iter().map(|(id, node)| {
            let body = node.render(
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
                .child(
                    div()
                        .bg(node.data.header_color())
                        .child(node.data.name()),
                )
                .child(body)
        });

        let all_nodes = self.node_registry.all().keys().cloned().collect::<Vec<_>>();
        div()
            .w_full()
            .h_full()
            .children(nodes)
            .on_action(cx.listener(Self::on_add_node_action))
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
