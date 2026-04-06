use std::{
    collections::HashMap,
    fs::read_to_string,
    ops::Deref,
    str::FromStr,
    sync::{Arc, LazyLock},
};

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
        Graph, GraphResources,
        external::{ExternalVariable, ExternalVariableId},
        function::{GraphFunction, GraphFunctionId, GraphFunctionStorage},
        node::GraphNodeRegistry,
        texture::{GraphTextureStorage, TextureId, TextureObject},
        variable::{GraphLiteral, GraphTypeRegistry},
    },
    save::{SerializableGraph, SerializableGraphFunction},
    wgsl_std::{
        builtin_nodes, builtin_types,
        nodes::{GraphInputNode, GraphOutputNode},
    },
};
use cyancia_widgets::drag_drop_column::DragDropColumn;
use iced_core::{
    Color, Element, Length,
    keyboard::{self, key},
    mouse, window,
};
use iced_runtime::{Task, futures::Subscription};
use iced_widget::{Column, button, column, container, row, space, text, text_input};
use parking_lot::RwLock;
use uuid::Uuid;
use wgpu::{Device, Queue};

use crate::{
    asset::{BrushPreset, BrushPresetMetadata, GpuImage, Image},
    browser::{ExternalVarViewMessage, brush_asset_browser, external_var_view},
    input_processing::InputProcessor,
    instance::{
        BrushPresetInstance, MAIN_GRAPH_NODES, REQUIRED_SPACING_GRAPH_NODES,
        SPACING_FACTOR_GRAPH_NODES, STROKE_POSTPROCESS_GRAPH_NODES,
    },
    render::BrushPresetOperator,
    tool::CurrentBrushPresetOperator,
};

pub struct SelectedBrush {
    pub asset_id: Option<AssetId<BrushPreset>>,
    pub instance: Arc<RwLock<BrushPresetInstance>>,
    pub viewing_graph: BrushPresetGraph,
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

const FUNCTION_GRAPH_NODE_REGISTRY: LazyLock<Arc<GraphNodeRegistry>> = LazyLock::new(|| {
    let mut registry = GraphNodeRegistry::default();

    registry.merge(builtin_nodes());
    registry.register::<GraphInputNode>();
    registry.register::<GraphOutputNode>();

    registry.into()
});

const FUNCTION_GRAPH_TYPE_REGISTRY: LazyLock<Arc<GraphTypeRegistry>> = LazyLock::new(|| {
    let mut registry = GraphTypeRegistry::default();

    registry.merge(builtin_types());

    registry.into()
});

pub struct BrushEditorView {
    input_manager: InputManager,
    texture_storage: Arc<GraphTextureStorage>,
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
                    func.deserialize_func(
                        FUNCTION_GRAPH_TYPE_REGISTRY.clone(),
                        FUNCTION_GRAPH_NODE_REGISTRY.as_ref(),
                    )
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
        let texture_storage = Arc::new(GraphTextureStorage::new(textures));

        let actions = runtime
            .service::<ActionManifestCollection>()
            .subset_for_view("brush_editor");

        Self {
            input_manager: InputManager::new(actions),

            selected: None,
            texture_storage,
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

#[derive(Clone)]
pub enum BrushPresetGraph {
    RequiredSpacing,
    SpacingFactor,
    Main,
    StrokePostprocess { index: usize },
}

impl BrushPresetGraph {
    pub fn graph<'a>(&self, brush: &'a BrushPresetInstance) -> &'a Graph {
        match self {
            BrushPresetGraph::RequiredSpacing => brush.required_spacing_graph(),
            BrushPresetGraph::SpacingFactor => brush.spacing_factor_graph(),
            BrushPresetGraph::Main => brush.main_graph(),
            BrushPresetGraph::StrokePostprocess { index } => {
                brush.stroke_postprocess_graphs().get(*index).unwrap()
            }
        }
    }

    pub fn node_registry(&self) -> Arc<GraphNodeRegistry> {
        match self {
            BrushPresetGraph::RequiredSpacing => REQUIRED_SPACING_GRAPH_NODES.clone(),
            BrushPresetGraph::SpacingFactor => SPACING_FACTOR_GRAPH_NODES.clone(),
            BrushPresetGraph::Main => MAIN_GRAPH_NODES.clone(),
            BrushPresetGraph::StrokePostprocess { index } => STROKE_POSTPROCESS_GRAPH_NODES.clone(),
        }
    }

    pub fn graph_mut<'a>(&self, brush: &'a mut BrushPresetInstance) -> &'a mut Graph {
        match self {
            BrushPresetGraph::RequiredSpacing => brush.required_spacing_graph_mut(),
            BrushPresetGraph::SpacingFactor => brush.spacing_factor_graph_mut(),
            BrushPresetGraph::Main => brush.main_graph_mut(),
            BrushPresetGraph::StrokePostprocess { index } => brush
                .stroke_postprocess_graphs_mut()
                .get_mut(*index)
                .unwrap(),
        }
    }
}

#[derive(Clone)]
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
    CreateNewStrokePostprocessGraph,
    SwitchToGraph(BrushPresetGraph),
    ReorderStrokePostprocessGraph { old_index: usize, new_index: usize },
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
                .map(|handle| {
                    let preset = handle.get().unwrap();
                    (handle.id(), preset)
                }),
            std::convert::identity,
        )
        .map(BrushEditorMessage::BrushSelected);

        let functions = Column::from_iter(
            self.function_storage
                .all()
                .iter()
                .map(|(id, func)| {
                    let id = *id;
                    Element::new(button(text(func.name.clone())).on_press_with(move || id))
                        .map(BrushEditorMessage::FunctionSelected)
                })
                .collect::<Vec<_>>(),
        );

        let mut editor = row![
            column![
                row![
                    Element::new(
                        button("New Brush").on_press(BrushEditorMessage::CreateNewBrushPreset)
                    ),
                    Element::new(
                        button("New Function").on_press(BrushEditorMessage::CreateNewFunction)
                    ),
                ],
                brushes,
                functions,
            ]
            .spacing(2),
        ];

        if let Some(selected) = &self.selected {
            let title_widget = if self.editing_name {
                Element::new(
                    container(
                        text_input("", &self.name_buffer)
                            .on_input(BrushEditorMessage::EditNameInput)
                            .on_submit(BrushEditorMessage::FinishEditName),
                    )
                    .height(24),
                )
            } else {
                let title = match selected {
                    Selected::Brush(brush) => &brush.instance.read().metadata().name.clone(),
                    Selected::Function(func) => &func.instance.name.clone(),
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
                    let instance = brush.instance.read();
                    let graph = brush.viewing_graph.graph(&instance);
                    let node_registry = brush.viewing_graph.node_registry();

                    let graph_view = Element::new(GraphView::new(&graph, &node_registry))
                        .map(BrushEditorMessage::GraphView);
                    // TODO: External var browser may not only be placed in editor. They're modifiable values for the user.
                    //       For example the brush size and opacity.
                    let ext_vars = external_var_view(
                        instance.iter_external_vars(),
                        instance.main_graph().type_registry(),
                        self.create_new_name.clone(),
                        self.create_new_type,
                    )
                    .map(BrushEditorMessage::ExternalVarView);

                    let graph_switcher = column![
                        button("New Stroke Postprocess Graph")
                            .on_press_with(|| BrushEditorMessage::CreateNewStrokePostprocessGraph),
                        button("Required Spacing").on_press_with(move || {
                            BrushEditorMessage::SwitchToGraph(BrushPresetGraph::RequiredSpacing)
                        }),
                        button("Spacing Factor").on_press_with(move || {
                            BrushEditorMessage::SwitchToGraph(BrushPresetGraph::SpacingFactor)
                        }),
                        button("Main").on_press_with(move || BrushEditorMessage::SwitchToGraph(
                            BrushPresetGraph::Main
                        )),
                    ]
                    .push(
                        DragDropColumn::with_children(
                            instance.stroke_postprocess_graphs().iter().enumerate().map(
                                |(index, graph)| {
                                    Element::new(
                                        button(text(format!("Stroke Postprocess {}", index)))
                                            .on_press_with(move || {
                                                BrushEditorMessage::SwitchToGraph(
                                                    BrushPresetGraph::StrokePostprocess { index },
                                                )
                                            }),
                                    )
                                },
                            ),
                        )
                        .on_drop(|ctx| {
                            Some(BrushEditorMessage::ReorderStrokePostprocessGraph {
                                old_index: ctx.item_index,
                                new_index: ctx.gap_index,
                            })
                        }),
                    );

                    editor = editor
                        .push(column![title_widget, graph_view])
                        .push(graph_switcher)
                        .push(ext_vars);
                }
                Selected::Function(func) => {
                    let graph = Element::new(GraphView::new(
                        &func.instance.graph,
                        FUNCTION_GRAPH_NODE_REGISTRY.as_ref(),
                    ))
                    .map(BrushEditorMessage::GraphView);

                    editor = editor.push(column![title_widget, graph]);
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
                                match brush.instance.read().compile(0) {
                                    Ok(compiled) => println!("Generated shader:\n{}", compiled),
                                    Err(e) => println!("Failed to generate shader: \n{:?}", e),
                                }
                            } else {
                                println!("No brush graph to generate shader from.");
                            }
                        }
                        if physical_key == key::Physical::Code(key::Code::KeyO)
                            && modifiers.control()
                        {
                            if let Some(Selected::Brush(brush)) = &mut self.selected {
                                println!(
                                    "{}",
                                    brush.instance.read().main_graph().to_toml().unwrap()
                                );
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
                                    let instance = brush.instance.read();
                                    let assets = runtime.service_mut::<AssetRegistry>();
                                    let preset = instance.as_asset().unwrap();
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
                                                format!("{}.cbp", instance.metadata().name),
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
                                            ctx.device.deref().clone(),
                                            ctx.queue.deref().clone(),
                                            InputProcessor::default(),
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

                match selected {
                    Selected::Brush(brush) => {
                        let mut instance = brush.instance.write();
                        let g = brush.viewing_graph.graph_mut(&mut instance);
                        Self::apply_graph_view_message(
                            g,
                            message,
                            &brush.viewing_graph.node_registry(),
                        );
                    }
                    Selected::Function(func) => {
                        Self::apply_graph_view_message(
                            &mut func.instance.graph,
                            message,
                            FUNCTION_GRAPH_NODE_REGISTRY.as_ref(),
                        );
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
                    &brush,
                    self.texture_storage.clone(),
                    self.function_storage.clone(),
                );

                if let Some(instance) = instance {
                    let instance = Arc::new(RwLock::new(instance));
                    self.selected = Some(Selected::Brush(SelectedBrush {
                        asset_id: Some(brush_id),
                        instance: instance.clone(),
                        viewing_graph: BrushPresetGraph::Main,
                    }));

                    let ctx = runtime.service::<RenderContext>();
                    runtime.insert_service(CurrentBrushPresetOperator::new(
                        BrushPresetOperator::new(
                            instance,
                            ctx.device.deref().clone(),
                            ctx.queue.deref().clone(),
                            InputProcessor::default(),
                        ),
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
                let (maybe_func, errs) = ser_func.deserialize_func(
                    FUNCTION_GRAPH_TYPE_REGISTRY.clone(),
                    FUNCTION_GRAPH_NODE_REGISTRY.as_ref(),
                );
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
                        graph: Graph::new(
                            GraphResources {
                                functions: self.function_storage.clone(),
                                ..Default::default()
                            },
                            FUNCTION_GRAPH_TYPE_REGISTRY.clone(),
                        ),
                    },
                }));
                self.has_unsaved_changes = true;
            }
            BrushEditorMessage::CreateNewBrushPreset => {
                let new_brush = BrushPreset {
                    metadata: BrushPresetMetadata {
                        name: "[Unnamed Brush]".to_string(),
                    },
                    spacing_factor_graph: SerializableGraph::default(),
                    required_spacing_graph: SerializableGraph::default(),
                    main_graph: SerializableGraph::default(),
                    stroke_postprocess_graphs: Vec::new(),
                    external_vars: Vec::new(),
                };
                let new_brush = Arc::new(new_brush);
                let assets = runtime.service_mut::<AssetRegistry>();
                let id = assets
                    .add_asset(
                        // TODO
                        BundleId::new(
                            Uuid::from_str("b92c20f6-8cdb-42b8-efae-a92705efd029").unwrap(),
                        ),
                        "unnamed_brush.cbp".to_string(),
                        new_brush.clone(),
                    )
                    .unwrap();
                let handle = assets.handle(id).unwrap();

                let (instance, _) = BrushPresetInstance::from_asset(
                    &handle,
                    self.texture_storage.clone(),
                    self.function_storage.clone(),
                );
                self.selected = Some(Selected::Brush(SelectedBrush {
                    asset_id: None,
                    instance: Arc::new(RwLock::new(instance.unwrap())),
                    viewing_graph: BrushPresetGraph::Main,
                }));
                self.has_unsaved_changes = true;
            }
            BrushEditorMessage::StartEditName => {
                self.editing_name = true;
                self.name_buffer = match &self.selected {
                    Some(Selected::Brush(brush)) => brush.instance.read().metadata().name.clone(),
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
                            brush.instance.write().metadata_mut().name = self.name_buffer.clone()
                        }
                        Selected::Function(func) => func.instance.name = self.name_buffer.clone(),
                    }
                    self.has_unsaved_changes = true;
                }
            }
            BrushEditorMessage::EditNameInput(name) => {
                self.name_buffer = name;
            }
            BrushEditorMessage::CreateNewStrokePostprocessGraph => {
                dbg!();
                let Some(Selected::Brush(brush)) = &mut self.selected else {
                    return Task::none();
                };

                let index = brush.instance.write().new_stroke_postprocess_graph();
                brush.viewing_graph = BrushPresetGraph::StrokePostprocess { index };
                dbg!();
            }
            BrushEditorMessage::SwitchToGraph(g) => {
                let Some(Selected::Brush(brush)) = &mut self.selected else {
                    return Task::none();
                };

                brush.viewing_graph = g;
                dbg!();
            }
            BrushEditorMessage::ReorderStrokePostprocessGraph {
                old_index,
                mut new_index,
            } => {
                if old_index == new_index {
                    return Task::none();
                }
                let Some(Selected::Brush(brush)) = &mut self.selected else {
                    return Task::none();
                };

                if new_index > old_index {
                    new_index -= 1;
                }

                let mut instance = brush.instance.write();
                let graph = instance.stroke_postprocess_graphs_mut().remove(old_index);
                instance
                    .stroke_postprocess_graphs_mut()
                    .insert(new_index, graph);
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
    fn apply_graph_view_message(
        graph: &mut Graph,
        message: GraphViewMessage,
        nodes: &GraphNodeRegistry,
    ) {
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
            GraphViewMessage::NodeCreateRequest(point, node_name) => {
                graph.add_boxed_node(point, nodes.get(node_name).unwrap());
            }
            GraphViewMessage::NodeUpdate(message) => {
                graph.update_node(message);
            }
        }
    }

    pub fn handle_external_var_update(&mut self, message: ExternalVarViewMessage) {
        match message {
            ExternalVarViewMessage::LiteralChanged(id, message) => {
                let Some(Selected::Brush(brush)) = self.selected.as_mut() else {
                    return;
                };

                let instance = brush.instance.read();
                instance.update_external_var(&id, message);
            }
            ExternalVarViewMessage::CreateNewNameChanged(name) => {
                self.create_new_name = name;
            }
            ExternalVarViewMessage::CreateNewSelectedType(t) => {
                self.create_new_type = Some(t);
            }
            ExternalVarViewMessage::RequestCreateNew => {
                let Some(Selected::Brush(brush)) = self.selected.as_ref() else {
                    return;
                };

                if self.create_new_name.is_empty() {
                    return;
                }

                let mut instance = brush.instance.write();

                let Some(ty) = self
                    .create_new_type
                    .and_then(|t| instance.main_graph().type_registry().get_type(t))
                else {
                    return;
                };

                let id = ExternalVariableId::new(Uuid::new_v4());
                let value = GraphLiteral::new_boxed(ty.default_literal(), ty.clone());
                instance.insert_external_var(ExternalVariable {
                    id,
                    name: self.create_new_name.clone(),
                    value,
                });

                self.create_new_name.clear();
                self.create_new_type = None;
            }
        }
    }
}
