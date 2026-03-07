use std::{collections::HashMap, fs::read_to_string, str::FromStr, sync::Arc};

use cyancia_actions::input_manager::InputManager;
use cyancia_assets::{
    asset::{AssetHandle, AssetId},
    bundle::BundleId,
    store::AssetRegistry,
};
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
        Graph, GraphDynamicInstancesStorage, GraphFunctionStorage,
        node::{
            external::{ExternalNode, ExternalVariable, ExternalVariableId},
            function::{GraphFunction, GraphFunctionId, GraphFunctionNode, functioning},
        },
        variable::GraphLiteral,
    },
    save::SerializableGraphFunction,
    wgsl_std::{
        nodes::{TextureId, TextureNode, TextureObject, TextureStorage, TextureUsageRecorder},
        std_storage,
    },
};
use iced_core::{
    Color, Element, Length,
    keyboard::{self, key},
    mouse, window,
};
use iced_runtime::{Task, futures::Subscription};
use iced_widget::{Column, button, column, container, row, space, text, text_input};
use uuid::Uuid;
use wgpu::{Device, Queue};

use crate::{
    asset::{BrushPreset, BrushPresetInstance, BrushPresetMetadata, GpuImage, Image},
    browser::{ExternalVarViewMessage, brush_asset_browser, external_var_view},
    render::{BrushPresetOperator, graph::brush_graph_storage},
    tool::CurrentBrushPresetOperator,
};

pub struct SelectedBrush {
    pub asset_id: Option<AssetId<BrushPreset>>,
    pub instance: Arc<BrushPresetInstance>,
}

pub struct SelectedFunction {
    pub asset_id: Option<AssetId<SerializableGraphFunction>>,
    pub id: GraphFunctionId,
    pub instance: GraphFunction,
}

pub enum Selected {
    Brush(SelectedBrush),
    Function(SelectedFunction),
}

pub struct BrushEditorView {
    input_manager: InputManager,
    main_graph_storage: GraphDynamicInstancesStorage,
    function_graph_storage: Arc<GraphDynamicInstancesStorage>,
    texture_storage: Arc<TextureStorage>,

    function_storage: Arc<GraphFunctionStorage>,
    function_id_to_asset: HashMap<GraphFunctionId, AssetHandle<SerializableGraphFunction>>,
    selected: Option<Selected>,

    create_new_name: String,
    create_new_type: Option<&'static str>,
    has_unsaved_changes: bool,
    editing_name: bool,
    name_buffer: String,
}

impl FromRuntime for BrushEditorView {
    fn from_runtime(runtime: &Services) -> Self {
        let function_graph_storage = {
            // TODO: Support using function inside function.
            let mut storage = GraphDynamicInstancesStorage::default();
            storage.merge(std_storage());
            storage.merge(functioning());
            Arc::new(storage)
        };

        let function_assets = runtime
            .service::<AssetRegistry>()
            .all_handles_of::<SerializableGraphFunction>()
            .unwrap();
        let functions = function_assets
            .iter()
            .map(|handle| {
                let func = handle.get().unwrap();
                // TODO err handling
                (
                    func.id,
                    func.deserialize_func(function_graph_storage.clone())
                        .0
                        .unwrap(),
                )
            })
            .collect();
        let function_storage = Arc::new(GraphFunctionStorage::new(functions));
        let function_id_to_asset = function_assets
            .into_iter()
            .map(|handle| (handle.get().unwrap().id, handle))
            .collect();

        // TODO: Update this storage when asset changes.
        let textures = runtime
            .service::<AssetRegistry>()
            .all_handles_of::<Image>()
            .unwrap()
            .into_iter()
            .map(|h| TextureObject {
                external_id: TextureId::new(*h.id()),
                name: h.get().unwrap().metadata.name.clone(),
            })
            .collect();
        let texture_storage = Arc::new(TextureStorage::new(textures));
        let main_graph_storage = {
            let mut storage = GraphDynamicInstancesStorage::default();
            storage.merge(std_storage());
            storage.merge(brush_graph_storage());
            storage
                .nodes
                .register_non_default(GraphFunctionNode::new(function_storage.clone()));
            storage
        };

        let actions = runtime
            .service::<ActionManifestCollection>()
            .subset_for_view("brush_editor");

        Self {
            input_manager: InputManager::new(actions),

            selected: None,
            main_graph_storage,
            texture_storage,
            function_graph_storage,
            function_storage,
            function_id_to_asset,

            create_new_name: String::new(),
            create_new_type: None,
            has_unsaved_changes: false,
            editing_name: false,
            name_buffer: String::new(),
        }
    }
}

pub enum BrushEditorMessage {
    KeyboardEvent(keyboard::Event),
    MouseEvent(mouse::Event),
    GraphView(GraphViewMessage),
    BrushSelected(AssetId<BrushPreset>),
    FunctionSelected(GraphFunctionId),
    ExternalVarView(ExternalVarViewMessage),
    CreateNewBrushPreset,
    CreateNewFunction,
    StartEditName,
    CancelEditName,
    FinishEditName,
    EditNameInput(String),
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

        let brushes = brush_asset_browser(
            presets
                .into_iter()
                // TODO: Notify failure
                .filter_map(|handle| handle.get().ok().map(|preset| (handle.id(), preset))),
            std::convert::identity,
        )
        .map(BrushEditorMessage::BrushSelected);

        let functions = Column::from_iter(
            self.function_storage
                .all()
                .iter()
                .map(|(id, func)| {
                    let id = *id;
                    Element::new(button(text(func.read().name.clone())).on_press_with(move || id))
                        .map(BrushEditorMessage::FunctionSelected)
                })
                .collect::<Vec<_>>(),
        );

        let mut editor = row![
            column![
                row![
                    // TODO: This is an ugly workaround
                    // TODO: Warn user if there are unsaved changes when creating new preset/function
                    Element::new(button("New Brush").on_press(()))
                        .map(|_| BrushEditorMessage::CreateNewBrushPreset),
                    Element::new(button("New Function").on_press(()))
                        .map(|_| BrushEditorMessage::CreateNewFunction),
                ],
                brushes,
                functions,
            ]
            .spacing(2),
        ];

        if let Some(selected) = &self.selected {
            let title_widget = if self.editing_name {
                #[derive(Clone)]
                enum InputMessage {
                    Input(String),
                    Finish,
                    Cancel,
                }

                Element::new(
                    container(
                        text_input("", &self.name_buffer)
                            .on_input(InputMessage::Input)
                            .on_submit(InputMessage::Finish),
                    )
                    .height(24),
                )
                .map(|m| match m {
                    InputMessage::Input(s) => BrushEditorMessage::EditNameInput(s),
                    InputMessage::Finish => BrushEditorMessage::FinishEditName,
                    InputMessage::Cancel => BrushEditorMessage::CancelEditName,
                })
            } else {
                let title = match selected {
                    Selected::Brush(brush) => &brush.instance.metadata().name,
                    Selected::Function(func) => &func.instance.name,
                };
                let title = if self.has_unsaved_changes {
                    format!("{} *", title)
                } else {
                    title.clone()
                };
                Element::new(
                    button(text(title).center().size(20).color(Color::WHITE))
                        .width(Length::Fill)
                        .height(24)
                        .style(|t: &iced_core::Theme, s: button::Status| button::Style {
                            background: Some(t.palette().background.into()),
                            ..Default::default()
                        })
                        .on_press(|| ()),
                )
                .map(|_| BrushEditorMessage::StartEditName)
            };

            match selected {
                Selected::Brush(brush) => {
                    let graph = Element::new(GraphView::new(&brush.instance.main_graph()))
                        .map(BrushEditorMessage::GraphView);
                    // TODO: External var browser may not only be placed in editor. They're modifiable values for the user.
                    //       For example the brush size and opacity.
                    let ext_vars = external_var_view(
                        brush.instance.external_vars(),
                        &self.main_graph_storage.types,
                        self.create_new_name.clone(),
                        self.create_new_type,
                    )
                    .map(BrushEditorMessage::ExternalVarView);

                    editor = editor.push(column![title_widget, graph,]).push(ext_vars);
                }
                Selected::Function(func) => {
                    let graph = Element::new(GraphView::new(&func.instance.graph))
                        .map(BrushEditorMessage::GraphView);

                    editor = editor.push(column![title_widget, graph,]);
                }
            }
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
                            if let Some(Selected::Brush(brush)) = &mut self.selected {
                                match brush.instance.compile() {
                                    Ok(compiled) => println!("Generated shader:\n{}", compiled),
                                    Err(e) => println!("Failed to generate shader: {:?}", e),
                                }
                            } else {
                                println!("No brush graph to generate shader from.");
                            }
                        }
                        if physical_key == key::Physical::Code(key::Code::KeyO)
                            && modifiers.control()
                        {
                            if let Some(Selected::Brush(brush)) = &mut self.selected {
                                println!("{}", brush.instance.main_graph().to_toml().unwrap());
                            } else {
                                println!("No brush graph to generate shader from.");
                            }
                        }
                        if physical_key == key::Physical::Code(key::Code::KeyS)
                            && modifiers.control()
                            && let Some(selected) = &mut self.selected
                        {
                            match selected {
                                Selected::Brush(brush) => {
                                    let assets = runtime.service_mut::<AssetRegistry>();
                                    let preset = brush.instance.as_asset().unwrap();
                                    if let Some(asset_id) = brush.asset_id {
                                        let handle = assets.handle(asset_id).unwrap();
                                        handle.update(preset).unwrap();
                                        handle.write().unwrap();
                                    } else {
                                        let new_id = assets
                                            .add_asset(
                                                BundleId::new(
                                                    // TODO
                                                    Uuid::from_str(
                                                        "b92c20f6-8cdb-42b8-efae-a92705efd029",
                                                    )
                                                    .unwrap(),
                                                ),
                                                format!("{}.cbp", brush.instance.metadata().name),
                                                Arc::new(preset),
                                            )
                                            .unwrap();
                                        brush.asset_id = Some(new_id);
                                    }
                                    self.has_unsaved_changes = false;

                                    let ctx = runtime.service::<RenderContext>();
                                    runtime.insert_service(CurrentBrushPresetOperator::new(
                                        BrushPresetOperator::new(
                                            brush.instance.clone(),
                                            ctx.device.clone(),
                                            ctx.queue.clone(),
                                        ),
                                    ));
                                }
                                Selected::Function(func) => {
                                    let assets = runtime.service_mut::<AssetRegistry>();
                                    let ser_func =
                                        SerializableGraphFunction::serialize_func(&func.instance)
                                            .unwrap();
                                    if let Some(asset_id) = func.asset_id {
                                        let handle = assets.handle(asset_id).unwrap();
                                        handle.update(ser_func).unwrap();
                                        handle.write().unwrap();
                                    } else {
                                        let new_id = assets
                                            .add_asset(
                                                // TODO
                                                BundleId::new(
                                                    Uuid::from_str(
                                                        "b92c20f6-8cdb-42b8-efae-a92705efd029",
                                                    )
                                                    .unwrap(),
                                                ),
                                                format!("{}.csf", func.instance.name),
                                                Arc::new(ser_func),
                                            )
                                            .unwrap();
                                        func.asset_id = Some(new_id);
                                    }
                                    self.has_unsaved_changes = false;
                                }
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
                // self.input_manager.on_mouse_event(event, &runtime);
            }
            BrushEditorMessage::GraphView(message) => {
                let Some(selected) = &mut self.selected else {
                    return Task::none();
                };
                let graph = match selected {
                    Selected::Brush(brush) => &mut brush.instance.main_graph_mut(),
                    Selected::Function(func) => &mut func.instance.graph,
                };

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
                self.has_unsaved_changes = true;

                // dbg!(self.texture_storage.used_textures());
            }
            BrushEditorMessage::BrushSelected(brush_id) => {
                let assets = runtime.service::<AssetRegistry>();
                let Ok(brush) = assets.handle(brush_id) else {
                    return Task::none();
                };
                let (instance, errors) = BrushPresetInstance::from_asset(
                    &brush.get().unwrap(),
                    self.texture_storage.clone(),
                    self.function_storage.clone(),
                );

                if let Some(instance) = instance {
                    let instance = Arc::new(instance);
                    self.selected = Some(Selected::Brush(SelectedBrush {
                        asset_id: Some(brush_id),
                        instance: instance.clone(),
                    }));

                    let ctx = runtime.service::<RenderContext>();
                    runtime.insert_service(CurrentBrushPresetOperator::new(
                        BrushPresetOperator::new(instance, ctx.device.clone(), ctx.queue.clone()),
                    ));
                }

                if !errors.is_empty() {
                    log::error!("Errors while loading brush preset:");
                    for error in errors {
                        log::error!("- {:?}", error);
                    }
                }
            }
            BrushEditorMessage::FunctionSelected(func_id) => {
                let Some(asset_handle) = self.function_id_to_asset.get(&func_id) else {
                    return Task::none();
                };
                let ser_func = asset_handle.get().unwrap();
                let (maybe_func, errs) =
                    ser_func.deserialize_func(self.function_graph_storage.clone());
                let Some(func) = maybe_func else {
                    for err in errs {
                        log::error!("Error deserializing function {:?}: {:?}", func_id, err);
                    }
                    return Task::none();
                };

                self.selected = Some(Selected::Function(SelectedFunction {
                    asset_id: Some(asset_handle.id()),
                    id: func_id,
                    instance: func,
                }));
            }
            BrushEditorMessage::ExternalVarView(message) => {
                self.handle_external_var_update(message);
            }
            BrushEditorMessage::CreateNewFunction => {
                self.selected = Some(Selected::Function(SelectedFunction {
                    asset_id: None,
                    id: GraphFunctionId::new(Uuid::new_v4()),
                    instance: GraphFunction {
                        id: GraphFunctionId::new(Uuid::new_v4()),
                        name: "[Unnamed Function]".to_string(),
                        graph: Graph::new(self.function_graph_storage.clone()),
                    },
                }));
                self.has_unsaved_changes = true;
            }
            BrushEditorMessage::CreateNewBrushPreset => {
                self.selected = Some(Selected::Brush(SelectedBrush {
                    asset_id: None,
                    instance: Arc::new(BrushPresetInstance::new(
                        BrushPresetMetadata {
                            name: "[Unnamed Brush]".to_string(),
                        },
                        self.texture_storage.clone(),
                        self.function_storage.clone(),
                    )),
                }));
                self.has_unsaved_changes = true;
            }
            BrushEditorMessage::StartEditName => {
                self.editing_name = true;
                self.name_buffer = match &self.selected {
                    Some(Selected::Brush(brush)) => brush.instance.metadata().name.clone(),
                    Some(Selected::Function(func)) => func.instance.name.clone(),
                    None => String::new(),
                };
            }
            BrushEditorMessage::CancelEditName => {
                self.editing_name = false;
            }
            BrushEditorMessage::FinishEditName => {
                self.editing_name = false;
                if let Some(selected) = &mut self.selected {
                    match selected {
                        Selected::Brush(brush) => {
                            brush.instance.metadata_mut().name = self.name_buffer.clone()
                        }
                        Selected::Function(func) => func.instance.name = self.name_buffer.clone(),
                    }
                    self.has_unsaved_changes = true;
                }
            }
            BrushEditorMessage::EditNameInput(name) => {
                self.name_buffer = name;
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
                let Some(Selected::Brush(brush)) = self.selected.as_mut() else {
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
                let Some(Selected::Brush(brush)) = self.selected.as_mut() else {
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
                    ExternalVariableId::new(Uuid::new_v4()),
                    ExternalVariable {
                        id: ExternalVariableId::new(Uuid::new_v4()),
                        name: self.create_new_name.clone(),
                        value: GraphLiteral::new_boxed(ty.default_literal(), ty.clone()),
                    },
                );

                self.create_new_name.clear();
                self.create_new_type = None;
            }
        }
    }
}
