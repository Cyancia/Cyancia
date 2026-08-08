use std::sync::LazyLock;

use cyancia_shader_graph::{
    editor::{GraphEditorView, GraphEditorMessage, GraphEditorState},
    graph::{
        Graph, GraphData, GraphResources, node::GraphNodeRegistry, variable::GraphTypeRegistry,
    },
    wgsl_std::{
        builtin_nodes, builtin_types,
        nodes::{GraphInputNode, GraphOutputNode},
    },
};

#[derive(Default, Clone)]
struct DemoData;

impl GraphData for DemoData {
    fn type_registry() -> &'static GraphTypeRegistry {
        LazyLock::force(&TYPE_REGISTRY)
    }

    fn node_registry() -> &'static GraphNodeRegistry<Self> {
        LazyLock::force(&NODE_REGISTRY)
    }
}

static NODE_REGISTRY: LazyLock<GraphNodeRegistry<DemoData>> = LazyLock::new(|| {
    let mut nodes = builtin_nodes();
    nodes.register::<GraphInputNode>();
    nodes.register::<GraphOutputNode>();
    nodes
});

static TYPE_REGISTRY: LazyLock<GraphTypeRegistry> = LazyLock::new(builtin_types);

struct DemoEditor {
    graph: Graph<DemoData>,
    editor_state: GraphEditorState,
}

impl DemoEditor {
    fn new() -> Self {
        Self {
            graph: Graph::new(GraphResources::default()),
            editor_state: GraphEditorState::default(),
        }
    }

    fn view(&self) -> iced_core::Element<'_, GraphEditorMessage, iced::Theme, iced_wgpu::Renderer> {
        GraphEditorView::new(&self.graph, &self.editor_state, false).into()
    }

    fn update(&mut self, message: GraphEditorMessage) {
        self.editor_state.update(&mut self.graph, message);
    }
}

fn main() -> iced::Result {
    iced::application(DemoEditor::new, DemoEditor::update, DemoEditor::view)
        .window_size((1280.0, 800.0))
        .run()
}
