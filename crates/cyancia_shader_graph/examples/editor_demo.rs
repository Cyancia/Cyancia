use cyancia_shader_graph::{
    editor::GraphEditor,
    graph::{Graph, GraphData, GraphResources},
    wgsl_std::{builtin_nodes, builtin_types},
};
use gpui::{AppContext, Context, Entity, IntoElement, ParentElement, Render, Styled, Window, div};
use gpui_component::Root;
use gpui_platform::application;

#[derive(Default, Clone)]
struct DemoData {}

impl GraphData for DemoData {}

struct DemoEditor {
    editor: Entity<GraphEditor<DemoData>>,
}

impl DemoEditor {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self {
            editor: cx.new(|_| {
                GraphEditor::new(
                    Graph::new(GraphResources::default().into(), builtin_types().into()),
                    builtin_nodes(),
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
    application().run(|cx| {
        gpui_component::init(cx);

        cx.open_window(Default::default(), |window, cx| {
            let editor = cx.new(|cx| DemoEditor::new(window, cx));

            cx.new(|cx| Root::new(editor, window, cx))
        });
    });
}
