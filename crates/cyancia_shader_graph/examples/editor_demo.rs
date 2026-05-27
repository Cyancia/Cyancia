use cyancia_shader_graph::{
    editor::GraphEditor,
    graph::{Graph, GraphData, GraphResources},
    wgsl_std::{
        builtin_nodes, builtin_types,
        nodes::{GraphInputNode, GraphOutputNode},
    },
};
use gpui::{AppContext, Context, Entity, IntoElement, ParentElement, Render, Styled, Window, div};
use gpui_component::{ActiveTheme, Root};
use gpui_platform::application;

#[derive(Default, Clone)]
struct DemoData {}

impl GraphData for DemoData {}

struct DemoEditor {
    editor: Entity<GraphEditor<DemoData>>,
}

impl DemoEditor {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let mut nodes = builtin_nodes();
        nodes.register::<GraphInputNode>();
        nodes.register::<GraphOutputNode>();
        Self {
            editor: cx.new(|cx| {
                GraphEditor::new(
                    Graph::new(GraphResources::default().into(), builtin_types().into()),
                    nodes.into(),
                    cx,
                )
            }),
        }
    }
}

impl Render for DemoEditor {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div().w_full().h_full().child(self.editor.clone())
    }
}

fn main() {
    application()
        .with_assets(gpui_component_assets::Assets)
        .run(|cx| {
            gpui_component::init(cx);
            cyancia_widgets::init(cx);

            cx.open_window(Default::default(), |window, cx| {
                let editor = cx.new(|cx| DemoEditor::new(window, cx));

                cx.new(|cx| Root::new(editor, window, cx))
            });
        });
}
