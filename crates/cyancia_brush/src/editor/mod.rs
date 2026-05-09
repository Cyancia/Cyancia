use std::{
    collections::HashMap,
    fs::read_to_string,
    ops::Deref,
    str::FromStr,
    sync::{Arc, LazyLock},
};

use cyancia_actions::actions_matcher::ActionsMatcher;
use cyancia_assets::{
    asset::{AssetHandle, AssetId},
    bundle::BundleId,
    store::AssetRegistry,
};
use cyancia_input::action::ActionManifestCollection;
use cyancia_render::texture::Image;
use cyancia_runtime::{
    Services,
    service::{FromServices, RenderContext},
    windows::{WindowView, WindowViewId},
};
use cyancia_shader_graph::{
    GraphRenderer, GraphTheme,
    editor::{GraphView, GraphViewMessage},
    graph::{
        Graph, GraphData, GraphResources,
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
    asset::{BrushPreset, BrushPresetMetadata},
    browser::{ExternalVarViewMessage, brush_asset_browser, external_var_view},
    input_processing::InputProcessor,
    instance::{
        BrushPresetInstance, GraphFunctionInstance, MAIN_GRAPH_NODES, REQUIRED_SPACING_GRAPH_NODES,
        SPACING_FACTOR_GRAPH_NODES, STROKE_POSTPROCESS_GRAPH_NODES,
    },
    render::{BrushPresetOperator, graph::{BrushGraphData, BrushGraphPostprocessData}},
    tool::CurrentBrushPresetOperator,
};

const FUNCTION_GRAPH_NODE_REGISTRY: LazyLock<Arc<GraphNodeRegistry<BrushGraphData>>> =
    LazyLock::new(|| {
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
    windows: Arc<[window::Id]>,
    main_window: window::Id,

    input_manager: ActionsMatcher,
    texture_storage: Arc<GraphTextureStorage>,
    main_function_storage: Arc<GraphFunctionStorage<BrushGraphData>>,
    stroke_pp_function_storage: Arc<GraphFunctionStorage<BrushGraphPostprocessData>>,

    function_id_to_asset: HashMap<GraphFunctionId, AssetHandle<SerializableGraphFunction>>,
    selected: Option<Selected>,

    saved_runtime_revision: u64,

    create_new_name: String,
    create_new_type: Option<&'static str>,
    editing_name: bool,
    name_buffer: String,
}

#[derive(Clone)]
pub enum BrushPresetGraph {
    RequiredSpacing,
    Main,
    StrokePostprocess { index: usize },
}

pub struct SelectedBrush {
    pub asset_id: Option<AssetId<BrushPreset>>,
    // TODO Refactor and remove this lock. In the future, we are selecting current brush preset by
    //      a preset dock, and editor can edit presets other than the current one.
    pub instance: Arc<RwLock<BrushPresetInstance>>,
    pub viewing_graph: BrushPresetGraph,
}

pub struct SelectedFunction {
    pub asset_id: Option<AssetId<SerializableGraphFunction>>,
    pub id: GraphFunctionId,
    pub instance: GraphFunctionInstance,
}

pub enum Selected {
    Brush(SelectedBrush),
    Function(SelectedFunction),
}

impl Selected {
    pub fn runtime_revision(&self) -> u64 {
        match self {
            Selected::Brush(brush) => brush.instance.read().runtime_revision(),
            Selected::Function(func) => func.instance.runtime_revision(),
        }
    }

    pub fn name(&self) -> String {
        match self {
            Selected::Brush(brush) => brush.instance.read().metadata().name.clone(),
            Selected::Function(func) => func.instance.graph_function().name.clone(),
        }
    }

    pub fn set_name(&mut self, name: String) {
        match self {
            Selected::Brush(brush) => brush.instance.write().metadata_mut().name = name,
            Selected::Function(func) => func.instance.graph_function_mut().name = name,
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

    fn id() -> WindowViewId {
        WindowViewId::new("brush_editor")
    }

    fn boot(services: &mut Services) -> (Self, Task<Self::Message>) {
        let function_assets = services
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
        let textures = services
            .service::<AssetRegistry>()
            .all_handles_of::<Image>()
            .unwrap();
        let texture_storage = Arc::new(GraphTextureStorage::new(textures));

        let actions = services
            .service::<ActionManifestCollection>()
            .subset_for_view("brush_editor");

        let (main_window, task) = iced_runtime::window::open(Default::default());

        (
            Self {
                windows: [main_window].into(),
                main_window,
                input_manager: ActionsMatcher::new(actions),

                selected: None,
                texture_storage,
                main_function_storage: function_storage,
                stroke_pp_function_storage: Arc::new(GraphFunctionStorage::new(HashMap::new())), // TODO

                function_id_to_asset,

                saved_runtime_revision: 0,

                create_new_name: String::new(),
                create_new_type: None,
                editing_name: false,
                name_buffer: String::new(),
            },
            task.discard(),
        )
    }

    fn view<'a>(
        &'a self,
        window: window::Id,
        services: &Services,
    ) -> impl Into<Element<'a, Self::Message, iced_core::Theme, iced_wgpu::Renderer>> {
        let assets = services.service::<AssetRegistry>();

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
            self.main_function_storage
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
                let title = if self.saved_runtime_revision != selected.runtime_revision() {
                    format!("{} *", selected.name())
                } else {
                    selected.name()
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

                    let graph_view = match brush.viewing_graph {
                        BrushPresetGraph::RequiredSpacing => Element::new(GraphView::new(
                            instance.required_spacing_graph(),
                            REQUIRED_SPACING_GRAPH_NODES.as_ref(),
                        )),
                        BrushPresetGraph::Main => Element::new(GraphView::new(
                            instance.main_graph(),
                            MAIN_GRAPH_NODES.as_ref(),
                        )),
                        BrushPresetGraph::StrokePostprocess { index } => {
                            Element::new(GraphView::new(
                                instance.stroke_postprocess_graphs().get(index).unwrap(),
                                STROKE_POSTPROCESS_GRAPH_NODES.as_ref(),
                            ))
                        }
                    }
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
                        button("Main").on_press_with(move || BrushEditorMessage::SwitchToGraph(
                            BrushPresetGraph::Main
                        )),
                    ]
                    .push(
                        DragDropColumn::with_children(
                            instance.stroke_postprocess_graphs().iter().enumerate().map(
                                |(index, _)| {
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
                        &func.instance.graph_function().graph,
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
        services: &mut Services,
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
                                    self.saved_runtime_revision = instance.runtime_revision();
                                    let assets = services.service_mut::<AssetRegistry>();
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

                                    let ctx = services.service::<RenderContext>();
                                    services.insert_service(CurrentBrushPresetOperator::new(
                                        BrushPresetOperator::new(
                                            brush.instance.clone(),
                                            ctx.device.deref().clone(),
                                            ctx.queue.deref().clone(),
                                            InputProcessor::default(),
                                        ),
                                    ));
                                }
                                Selected::Function(func) => {
                                    let assets = services.service_mut::<AssetRegistry>();
                                    let ser_func = SerializableGraphFunction::serialize_func(
                                        func.instance.graph_function(),
                                    )
                                    .unwrap();
                                    self.saved_runtime_revision = func.instance.runtime_revision();
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
                                                format!(
                                                    "{}.csf",
                                                    func.instance.graph_function().name
                                                ),
                                                Arc::new(ser_func),
                                            )
                                            .unwrap();
                                        func.asset_id = Some(new_id);
                                    }
                                }
                            }

                            log::info!("Saved current item.")
                        }
                    }
                    _ => {}
                }

                // return self
                //     .input_manager
                //     .on_keyboard_event(event, runtime)
                //     .discard();
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
                        match brush.viewing_graph {
                            BrushPresetGraph::RequiredSpacing => Self::apply_graph_view_message(
                                instance.required_spacing_graph_mut(),
                                message,
                                REQUIRED_SPACING_GRAPH_NODES.as_ref(),
                            ),
                            BrushPresetGraph::Main => Self::apply_graph_view_message(
                                instance.main_graph_mut(),
                                message,
                                MAIN_GRAPH_NODES.as_ref(),
                            ),
                            BrushPresetGraph::StrokePostprocess { index } => {
                                Self::apply_graph_view_message(
                                    instance
                                        .stroke_postprocess_graphs_mut()
                                        .get_mut(index)
                                        .unwrap(),
                                    message,
                                    STROKE_POSTPROCESS_GRAPH_NODES.as_ref(),
                                )
                            }
                        }
                    }
                    Selected::Function(func) => {
                        Self::apply_graph_view_message(
                            &mut func.instance.graph_function_mut().graph,
                            message,
                            FUNCTION_GRAPH_NODE_REGISTRY.as_ref(),
                        );
                    }
                }

                // dbg!(self.texture_storage.used_textures());
            }
            BrushEditorMessage::BrushSelected(brush_id) => {
                let assets = services.service::<AssetRegistry>();
                let Ok(brush) = assets.handle(brush_id) else {
                    return Task::none();
                };
                let (instance, errors) = BrushPresetInstance::from_asset(
                    &brush,
                    self.texture_storage.clone(),
                    self.main_function_storage.clone(),
                    self.stroke_pp_function_storage.clone(),

                );

                if let Some(instance) = instance {
                    let instance = Arc::new(RwLock::new(instance));
                    self.selected = Some(Selected::Brush(SelectedBrush {
                        asset_id: Some(brush_id),
                        instance: instance.clone(),
                        viewing_graph: BrushPresetGraph::Main,
                    }));

                    let ctx = services.service::<RenderContext>();
                    services.insert_service(CurrentBrushPresetOperator::new(
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
                    instance: GraphFunctionInstance::new(func),
                }));
            }
            BrushEditorMessage::ExternalVarView(message) => {
                self.handle_external_var_update(message);
            }
            BrushEditorMessage::CreateNewFunction => {
                let id = GraphFunctionId::new(Uuid::new_v4());
                self.selected = Some(Selected::Function(SelectedFunction {
                    asset_id: None,
                    id,
                    instance: GraphFunctionInstance::new(GraphFunction {
                        id,
                        name: "[Unnamed Function]".to_string(),
                        graph: Graph::new(
                            GraphResources {
                                functions: self.main_function_storage.clone(),
                                ..Default::default()
                            },
                            FUNCTION_GRAPH_TYPE_REGISTRY.clone(),
                        ),
                    }),
                }));
                self.saved_runtime_revision = 0;
            }
            BrushEditorMessage::CreateNewBrushPreset => {
                let new_brush = BrushPreset {
                    metadata: BrushPresetMetadata {
                        name: "[Unnamed Brush]".to_string(),
                    },
                    required_spacing_graph: SerializableGraph::default(),
                    main_graph: SerializableGraph::default(),
                    stroke_postprocess_graphs: Vec::new(),
                    external_vars: Vec::new(),
                };
                let new_brush = Arc::new(new_brush);
                let assets = services.service_mut::<AssetRegistry>();
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
                    self.main_function_storage.clone(),
                    self.stroke_pp_function_storage.clone(),
                );
                self.selected = Some(Selected::Brush(SelectedBrush {
                    asset_id: None,
                    instance: Arc::new(RwLock::new(instance.unwrap())),
                    viewing_graph: BrushPresetGraph::Main,
                }));
                self.saved_runtime_revision = 0;
            }
            BrushEditorMessage::StartEditName => {
                self.editing_name = true;
                self.name_buffer = match &self.selected {
                    Some(selected) => selected.name(),
                    None => String::new(),
                };
            }
            BrushEditorMessage::CancelEditName => {
                self.editing_name = false;
            }
            BrushEditorMessage::FinishEditName => {
                self.editing_name = false;
                if let Some(selected) = &mut self.selected {
                    selected.set_name(self.name_buffer.clone());
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

    fn subscription(&self) -> Subscription<Self::Message> {
        iced_futures::event::listen_with(|event, _, window| {
            match event {
                iced_core::Event::Keyboard(event) => Some(BrushEditorMessage::KeyboardEvent(event)),
                iced_core::Event::Mouse(event) => Some(BrushEditorMessage::MouseEvent(event)),
                _ => None,
            }
            .map(|msg| (msg, window))
        })
        .with(self.main_window)
        .filter_map(|(main_window_id, (msg, window))| {
            if window == main_window_id {
                Some(msg)
            } else {
                None
            }
        })
    }

    fn close(self, services: &mut Services) -> Task<()> {
        iced_runtime::window::close(self.main_window)
    }

    fn windows(&self) -> Arc<[window::Id]> {
        self.windows.clone()
    }

    fn root_window(&self) -> Option<window::Id> {
        Some(self.main_window)
    }
}

impl BrushEditorView {
    fn apply_graph_view_message<Data: GraphData>(
        graph: &mut Graph<Data>,
        message: GraphViewMessage,
        nodes: &GraphNodeRegistry<Data>,
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
            ExternalVarViewMessage::RequestRemove(id) => {
                let Some(Selected::Brush(brush)) = self.selected.as_ref() else {
                    return;
                };

                let mut instance = brush.instance.write();
                instance.remove_external_var(&id);
            }
        }
    }
}
