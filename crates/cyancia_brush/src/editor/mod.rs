use std::{fs::read_to_string, sync::Arc};

use cyancia_assets::{asset::AssetId, store::AssetRegistry};
use cyancia_runtime::{
    Services,
    service::{FromRuntime, RenderContext},
    windows::{WindowView, WindowViewId},
};
use cyancia_shader_graph::{
    GraphRenderer, GraphTheme,
    editor::{GraphView, GraphViewMessage},
    graph::{Graph, GraphDynamicInstancesStorage, node::function::functioning},
    wgsl_std::std_storage,
};
use iced_core::{
    Element,
    keyboard::{self, key},
    mouse, window,
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
    pub id: AssetId<BrushPreset>,
    pub instance: BrushPresetInstance,
}

pub struct BrushEditorView {
    main_graph_storage: Arc<GraphDynamicInstancesStorage>,
    function_graph_storage: Arc<GraphDynamicInstancesStorage>,
    selected: Option<SelectedBrush>,
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
    BrushSelected(AssetId<BrushPreset>),
}

impl WindowView for BrushEditorView {
    type Message = BrushEditorMessage;

    fn id(&self) -> WindowViewId {
        WindowViewId::new("brush_editor")
    }

    fn view<'a>(
        &'a self,
        runtime: Arc<cyancia_runtime::Services>,
    ) -> impl Into<Element<'a, Self::Message, iced_core::Theme, iced_wgpu::Renderer>> {
        let Ok(assets) = runtime
            .service::<AssetRegistry>()
            .all_handles_of::<BrushPreset>()
        else {
            return None;
        };

        let mut editor = row![
            brush_asset_browser(
                assets
                    .into_iter()
                    // TODO: Notify failure
                    .filter_map(|handle| handle.get().ok().map(|preset| (handle.id(), preset))),
                std::convert::identity
            )
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
        runtime: Arc<Services>,
    ) -> impl Into<Task<Self::Message>> {
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
                let assets = runtime.service::<AssetRegistry>();
                let Ok(brush) = assets.handle(brush_id) else {
                    return Task::none();
                };
                let render_context = runtime.service::<RenderContext>();

                let (instance, errors) = BrushPresetInstance::from_asset(
                    &brush.get().unwrap(),
                    self.main_graph_storage.clone(),
                    self.function_graph_storage.clone(),
                    &render_context.device,
                    &render_context.queue,
                );

                if let Some(instance) = instance {
                    self.selected = Some(SelectedBrush {
                        id: brush_id,
                        instance,
                    });
                }

                if !errors.is_empty() {
                    log::error!("Errors while loading brush preset:");
                    for error in errors {
                        log::error!("- {:?}", error);
                    }
                }
            }
            _ => {}
        }

        Task::none()
    }

    fn subscription(&self) -> Subscription<(window::Id, BrushEditorMessage)> {
        iced_futures::event::listen_with(|event, _, window| match event {
            iced_core::Event::Keyboard(event) => {
                Some((window, BrushEditorMessage::KeyboardEvent(event)))
            }
            iced_core::Event::Mouse(event) => Some((window, BrushEditorMessage::MouseEvent(event))),
            _ => None,
        })
    }
}
