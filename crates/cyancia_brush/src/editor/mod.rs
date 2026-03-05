use std::{fs::read_to_string, sync::Arc};

use cyancia_actions::input_manager::InputManager;
use cyancia_assets::{asset::AssetId, store::AssetRegistry};
use cyancia_input::action::ActionManifestCollection;
use cyancia_runtime::{
    Services,
    service::{FromRuntime, RenderContext},
    windows::{WindowView, WindowViewId},
};
use cyancia_shader_graph::{
    GraphRenderer, GraphTheme,
    editor::{GraphView, GraphViewMessage},
    graph::{
        Graph, GraphDynamicInstancesStorage,
        node::{
            external::{ExternalLiteralId, ExternalNode},
            function::functioning,
        },
        variable::GraphLiteral,
    },
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
    asset::{BrushPreset, BrushPresetInstance, GpuImage, Image},
    browser::{ExternalVarViewMessage, brush_asset_browser, external_var_view},
    render::graph::{brush_graph_storage, generate_brush_shader},
};

pub struct SelectedBrush {
    pub id: AssetId<BrushPreset>,
    pub instance: BrushPresetInstance,
}

pub struct BrushEditorView {
    input_manager: InputManager,
    main_graph_storage: GraphDynamicInstancesStorage,
    function_graph_storage: GraphDynamicInstancesStorage,
    selected: Option<SelectedBrush>,

    create_new_name: String,
    create_new_type: Option<&'static str>,
}

impl FromRuntime for BrushEditorView {
    fn from_runtime(runtime: &Services) -> Self {
        let main_graph_storage = {
            let mut storage = GraphDynamicInstancesStorage::default();
            storage.merge(std_storage());
            storage.merge(brush_graph_storage());
            storage
        };

        let function_graph_storage = {
            let mut storage = GraphDynamicInstancesStorage::default();
            storage.merge(std_storage());
            storage.merge(functioning());
            storage
        };

        let actions = runtime
            .service::<ActionManifestCollection>()
            .subset_for_view("brush_editor");

        let assets = runtime.service::<AssetRegistry>();
        // TODO: Update if assets change
        let images = assets.all_handles_of::<Image>().unwrap();

        Self {
            input_manager: InputManager::new(actions),

            selected: None,
            main_graph_storage,
            function_graph_storage,

            create_new_name: String::new(),
            create_new_type: None,
        }
    }
}

pub enum BrushEditorMessage {
    KeyboardEvent(keyboard::Event),
    MouseEvent(mouse::Event),
    GraphView(GraphViewMessage),
    BrushSelected(AssetId<BrushPreset>),
    ExternalVarView(ExternalVarViewMessage),
}

impl WindowView for BrushEditorView {
    type Message = BrushEditorMessage;

    fn id(&self) -> WindowViewId {
        WindowViewId::new("brush_editor")
    }

    fn view<'a>(
        &'a self,
        runtime: Arc<Services>,
    ) -> impl Into<Element<'a, Self::Message, iced_core::Theme, iced_wgpu::Renderer>> {
        let assets = runtime.service::<AssetRegistry>();

        let Ok(presets) = assets.all_handles_of::<BrushPreset>() else {
            return None;
        };

        let mut editor = row![
            brush_asset_browser(
                presets
                    .into_iter()
                    // TODO: Notify failure
                    .filter_map(|handle| handle.get().ok().map(|preset| (handle.id(), preset))),
                std::convert::identity
            )
            .map(BrushEditorMessage::BrushSelected)
        ];

        if let Some(brush) = &self.selected {
            editor = editor.push(
                Element::new(GraphView::new(&brush.instance.main_graph()))
                    .map(BrushEditorMessage::GraphView),
            );
            editor = editor.push(
                external_var_view(
                    brush.instance.external_vars(),
                    &self.main_graph_storage.types,
                    self.create_new_name.clone(),
                    self.create_new_type,
                )
                .map(BrushEditorMessage::ExternalVarView),
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
            BrushEditorMessage::KeyboardEvent(event) => {
                match event {
                    keyboard::Event::KeyPressed {
                        physical_key,
                        modifiers,
                        ..
                    } => {
                        // Just for debugging purpose :)
                        if physical_key == key::Physical::Code(key::Code::KeyP)
                            && modifiers.control()
                        {
                            if let Some(brush) = &mut self.selected {
                                match brush.instance.compile() {
                                    Ok(shader) => println!("Generated shader:\n{}", shader),
                                    Err(e) => println!("Failed to generate shader: {:?}", e),
                                }
                            } else {
                                println!("No brush graph to generate shader from.");
                            }
                        }
                    }
                    _ => {}
                }

                return self
                    .input_manager
                    .on_keyboard_event(event, runtime)
                    .discard();
            }
            BrushEditorMessage::MouseEvent(event) => {
                self.input_manager.on_mouse_event(event, &runtime);
            }
            BrushEditorMessage::GraphView(message) => {
                let Some(brush) = &mut self.selected else {
                    return Task::none();
                };
                let graph = brush.instance.main_graph_mut();

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

                // dbg!(self.texture_storage.used_textures());
            }
            BrushEditorMessage::BrushSelected(brush_id) => {
                let assets = runtime.service::<AssetRegistry>();
                let Ok(brush) = assets.handle(brush_id) else {
                    return Task::none();
                };
                let (instance, errors) = BrushPresetInstance::from_asset(
                    &brush.get().unwrap(),
                    self.main_graph_storage.clone(),
                    self.function_graph_storage.clone(),
                    runtime.service::<AssetRegistry>().as_ref(),
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
            BrushEditorMessage::ExternalVarView(message) => {
                self.handle_external_var_update(message);
            }
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

impl BrushEditorView {
    pub fn handle_external_var_update(&mut self, message: ExternalVarViewMessage) {
        match message {
            ExternalVarViewMessage::LiteralChanged(id, message) => {
                let Some(brush) = self.selected.as_mut() else {
                    return;
                };

                let ext_vars = brush.instance.external_vars();
                ext_vars.update(id, message);
            }
            ExternalVarViewMessage::CreateNewNameChanged(name) => {
                self.create_new_name = name;
            }
            ExternalVarViewMessage::CreateNewSelectedType(t) => {
                self.create_new_type = Some(t);
            }
            ExternalVarViewMessage::RequestCreateNew => {
                let Some(brush) = self.selected.as_mut() else {
                    return;
                };

                if self.create_new_name.is_empty() {
                    return;
                }

                let Some(ty) = self
                    .create_new_type
                    .and_then(|t| self.main_graph_storage.types.get(t))
                else {
                    return;
                };

                brush.instance.external_vars().insert(
                    ExternalLiteralId::new(self.create_new_name.clone()),
                    GraphLiteral::new_boxed(ty.default_literal(), ty.clone()),
                );

                self.create_new_name.clear();
                self.create_new_type = None;
            }
        }
    }
}
