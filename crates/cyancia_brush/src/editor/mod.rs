use std::{fs::read_to_string, sync::Arc};

use cyancia_id::Id;
use cyancia_shader_graph::{
    GraphRenderer, GraphTheme,
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
use iced_widget::space;

use crate::render::graph::{brush_graph_storage, generate_brush_shader};

pub struct BrushEditorView {
    main_graph_storage: Arc<GraphDynamicInstancesStorage>,
    function_graph_storage: Arc<GraphDynamicInstancesStorage>,
    // TODO: replace with entire preset
    brush: Option<Graph>,
}

impl Default for BrushEditorView {
    fn default() -> Self {
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
            brush: None,
            main_graph_storage,
            function_graph_storage,
        }
    }
}

pub enum BrushEditorMessage {
    KeyboardEvent(keyboard::Event),
    MouseEvent(mouse::Event),
}

impl WindowView<GraphTheme, GraphRenderer> for BrushEditorView {
    type Message = BrushEditorMessage;

    fn id(&self) -> Id<Window> {
        Id::from_str("brush_editor")
    }

    fn view(&self) -> Element<'static, Self::Message, GraphTheme, GraphRenderer> {
        space().into()
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
                    if let Some(graph) = &mut self.brush {
                        match generate_brush_shader(graph) {
                            Ok(shader) => println!("Generated shader:\n{}", shader),
                            Err(e) => println!("Failed to generate shader: {:?}", e),
                        }
                    } else {
                        println!("No brush graph to generate shader from.");
                    }
                }
                if physical_key == key::Physical::Code(key::Code::KeyO) && modifiers.control() {
                    let Some(file) = rfd::FileDialog::new()
                        .add_filter("Brush Graph", &["csg", "csf"])
                        .pick_file()
                    else {
                        return Task::none();
                    };

                    let storage = match file.extension().and_then(|e| e.to_str()) {
                        Some("csg") => self.main_graph_storage.clone(),
                        Some("csf") => self.function_graph_storage.clone(),
                        _ => {
                            println!("Unsupported file type.");
                            return Task::none();
                        }
                    };

                    let graph = Graph::from_toml(storage, &read_to_string(&file).unwrap())
                        .0
                        .unwrap();
                    self.brush = Some(graph);
                    println!("Loaded brush graph from file: {}", file.display());
                }
            }
            BrushEditorMessage::MouseEvent(event) => {}
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
