use std::{fs::read_to_string, sync::Arc};

use cyancia_assets::store::AssetRegistry;
use cyancia_id::Id;
use cyancia_shader_graph::{
    GraphRenderer, GraphTheme,
    editor::{GraphView, GraphViewMessage},
    graph::{Graph, GraphDynamicInstancesStorage, node::function::functioning},
    wgsl_std::std_storage,
};
use cyancia_windows::{Window, WindowManagerShell, WindowView};
use iced_core::{
    Element,
    keyboard::{self, key},
    mouse,
};
use iced_runtime::{Task, futures::Subscription};
use iced_widget::{container, row, space};
use wgpu::{Device, Queue};

use crate::{
    asset::{BrushPreset, BrushPresetInstance},
    browser::brush_asset_browser,
    render::graph::{brush_graph_storage, generate_brush_shader},
};

pub struct SelectedBrush {
    pub id: Id<BrushPreset>,
    pub instance: BrushPresetInstance,
}

pub struct BrushEditorView {
    assets: Arc<AssetRegistry>,
    main_graph_storage: Arc<GraphDynamicInstancesStorage>,
    function_graph_storage: Arc<GraphDynamicInstancesStorage>,
    selected: Option<SelectedBrush>,
}

impl BrushEditorView {
    pub fn new(assets: Arc<AssetRegistry>) -> Self {
        let main_graph_storage = {
            let mut storage = GraphDynamicInstancesStorage::default();
            storage.merge(std_storage());
            storage.merge(brush_graph_storage());
            Arc::new(storage)
        };

        let function_graph_storage = {
            let mut storage = GraphDynamicInstancesStorage::default();
            storage.merge(std_storage());
            storage.merge(functioning());
            Arc::new(storage)
        };

        Self {
            assets,
            selected: None,
            main_graph_storage,
            function_graph_storage,
        }
    }
}

pub enum BrushEditorMessage {
    KeyboardEvent(keyboard::Event),
    MouseEvent(mouse::Event),
    GraphView(GraphViewMessage),
    BrushSelected(Id<BrushPreset>),
}

impl WindowView<GraphTheme, GraphRenderer> for BrushEditorView {
    type Message = BrushEditorMessage;

    fn id(&self) -> Id<Window> {
        Id::from_str("brush_editor")
    }

    fn view<'a>(&'a self) -> Element<'a, Self::Message, GraphTheme, GraphRenderer> {
        let mut editor = row![
            brush_asset_browser(self.assets.store::<BrushPreset>(), std::convert::identity)
                .map(BrushEditorMessage::BrushSelected)
        ];

        if let Some(brush) = &self.selected {
            editor = editor.push(
                Element::new(GraphView::new(&brush.instance.main_graph))
                    .map(BrushEditorMessage::GraphView),
            );
        }

        editor.into()
    }

    fn update(
        &mut self,
        message: Self::Message,
        windows: &mut WindowManagerShell,
    ) -> Task<Self::Message> {
        match message {
            BrushEditorMessage::KeyboardEvent(keyboard::Event::KeyPressed {
                physical_key,
                modifiers,
                ..
            }) => {
                // TODO: with custom keybinds and actions.
                if physical_key == key::Physical::Code(key::Code::KeyP) && modifiers.control() {
                    if let Some(brush) = &mut self.selected {
                        match generate_brush_shader(&mut brush.instance.main_graph) {
                            Ok(shader) => println!("Generated shader:\n{}", shader),
                            Err(e) => println!("Failed to generate shader: {:?}", e),
                        }
                    } else {
                        println!("No brush graph to generate shader from.");
                    }
                }
            }
            BrushEditorMessage::MouseEvent(event) => {}
            BrushEditorMessage::GraphView(message) => {
                let Some(brush) = &mut self.selected else {
                    return Task::none();
                };
                let graph = &mut brush.instance.main_graph;

                match message {
                    GraphViewMessage::NodeMoveRequest(point, id) => {
                        if let Some(node) = graph.get_node_mut(&id) {
                            node.position = point;
                        }
                    }
                    GraphViewMessage::EdgeCreateRequest(from, to) => {
                        graph.connect_slots(from, to);
                    }
                    GraphViewMessage::EdgeRemoveRequest(id) => {
                        graph.disconnect_slot(id);
                    }
                    GraphViewMessage::NodeDeleteRequest(id) => {
                        graph.delete_node(&id);
                    }
                    GraphViewMessage::NodeCreateRequest(point, node) => {
                        graph.add_boxed_node(point, node);
                    }
                    GraphViewMessage::NodeUpdate(message) => {
                        graph.update_node(message);
                    }
                }
            }
            BrushEditorMessage::BrushSelected(brush_id) => {
                let Some(brush) = self.assets.asset(brush_id) else {
                    return Task::none();
                };

                println!("Selected brush: {}", brush.metadata.name);
                // BrushPresetInstance::from_asset(
                //     &brush,
                //     self.main_graph_storage.clone(),
                //     self.function_graph_storage.clone(),
                //     &self.device,
                //     &self.queue,
                // );
            }
            _ => {}
        }

        Task::none()
    }

    fn subscription(&self) -> Subscription<Self::Message> {
        iced_futures::event::listen().filter_map(|event| match event {
            iced_core::Event::Keyboard(event) => Some(BrushEditorMessage::KeyboardEvent(event)),
            iced_core::Event::Mouse(event) => Some(BrushEditorMessage::MouseEvent(event)),
            _ => None,
        })
    }
}
