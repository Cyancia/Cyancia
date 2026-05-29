use std::{
    collections::HashMap,
    fs::read_to_string,
    ops::Deref,
    rc::Rc,
    str::FromStr,
    sync::{Arc, LazyLock},
};

use cyancia_assets::{
    AssetAppExt,
    asset::{AssetHandle, AssetId},
    bundle::BundleId,
    store::AssetRegistry,
};
use cyancia_render::{render_context::RenderContext, texture::Image};
use cyancia_shader_graph::{
    editor::{GraphEditSink, GraphEditor},
    graph::{
        Graph, GraphData, GraphResources,
        external::{ExternalVariable, ExternalVariableId},
        function::{GraphFunction, GraphFunctionId, GraphFunctionStorage},
        node::GraphNodeRegistry,
        slot::GraphInlineLiteralRenderContext,
        texture::{GraphTextureStorage, TextureId, TextureObject},
        variable::{GraphLiteral, GraphTypeRegistry},
    },
    save::{SerializableGraph, SerializableGraphFunction},
    wgsl_std::{
        builtin_nodes, builtin_types,
        nodes::{GraphInputNode, GraphOutputNode},
    },
};
use gpui::{
    App, AppContext, Axis, Context, Entity, IntoElement, KeyBinding, ParentElement, Render, Styled,
    Window, actions, div, px, relative,
};
use gpui_component::{
    button::Button,
    form::{field, v_form},
    h_flex,
    input::{Input, InputEvent, InputState},
    list::{List, ListDelegate, ListEvent, ListState},
    select::{SearchableVec, Select, SelectState},
    v_flex,
};
use parking_lot::RwLock;
use uuid::Uuid;
use wgpu::{Device, Queue};

use crate::{
    asset::{BrushPreset, BrushPresetMetadata},
    input_processing::InputProcessor,
    instance::{
        BRUSH_GRAPH_TYPES, BrushPresetInstance, GraphFunctionInstance, MAIN_GRAPH_NODES,
        REQUIRED_SPACING_GRAPH_NODES, SPACING_FACTOR_GRAPH_NODES, STROKE_POSTPROCESS_GRAPH_NODES,
    },
    render::{
        BrushPresetOperator,
        graph::{BrushGraphData, BrushGraphPostprocessData},
    },
    tool::CurrentBrushPresetOperator,
    widget::{BrushFunctionListDelegate, BrushPresetListDelegate},
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

pub const BRUSH_EDITOR_CONTEXT: &str = "brush_editor";

actions!([SaveCurrentItem]);

pub(crate) fn init(cx: &mut App) {
    cx.bind_keys([KeyBinding::new(
        "ctrl-s",
        SaveCurrentItem,
        Some(BRUSH_EDITOR_CONTEXT),
    )]);
}

pub struct BrushEditor {
    texture_storage: Arc<GraphTextureStorage>,
    main_function_storage: Arc<GraphFunctionStorage<BrushGraphData>>,
    stroke_pp_function_storage: Arc<GraphFunctionStorage<BrushGraphPostprocessData>>,

    selected: Option<Selected>,

    saved_runtime_revision: u64,
    editor_state: Entity<GraphEditor>,
    brushes: Entity<ListState<BrushPresetListDelegate>>,
    functions: Entity<ListState<BrushFunctionListDelegate>>,
    name_input_state: Entity<InputState>,
    new_ext_var_name_input_state: Entity<InputState>,
    new_ext_var_type_select_state: Entity<SelectState<Vec<&'static str>>>,
    pane_selection: PaneSelection,
}

impl BrushEditor {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let brush_assets = cx.assets().all_handles_of::<BrushPreset>().unwrap();
        let function_assets = cx
            .assets()
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
                        Some(handle.id()),
                        FUNCTION_GRAPH_TYPE_REGISTRY.clone(),
                        FUNCTION_GRAPH_NODE_REGISTRY.as_ref(),
                    )
                    .0
                    .unwrap(),
                )
            })
            .collect();
        let function_storage = Arc::new(GraphFunctionStorage::new(functions));

        // TODO: Update this storage when asset changes.
        let textures = cx.assets().all_handles_of::<Image>().unwrap();
        let texture_storage = Arc::new(GraphTextureStorage::new(textures));

        let editor_state = cx.new(|cx| GraphEditor::new(cx));
        let brushes =
            cx.new(|cx| ListState::new(BrushPresetListDelegate::new(brush_assets), window, cx));
        let functions = cx
            .new(|cx| ListState::new(BrushFunctionListDelegate::new(function_assets), window, cx));
        let name_input_state = cx.new(|cx| InputState::new(window, cx));

        cx.subscribe_in(
            &brushes,
            window,
            |editor, brushes_entity, event: &ListEvent, window, cx| match event {
                ListEvent::Select(_) => {}
                ListEvent::Confirm(ix) => {
                    let Some(brush) = brushes_entity.update(cx, |brushes, cx| {
                        let item = brushes.delegate().get(*ix)?;
                        Some(item.handle.clone())
                    }) else {
                        return;
                    };

                    let (maybe_instance, errs) = BrushPresetInstance::from_asset(
                        &brush,
                        editor.texture_storage.clone(),
                        editor.main_function_storage.clone(),
                        editor.stroke_pp_function_storage.clone(),
                    );

                    if !errs.is_empty() {
                        for err in errs {
                            log::error!("Error deserializing brush preset {}: {}", brush.id(), err);
                        }
                    }

                    let Some(instance) = maybe_instance else {
                        log::error!("Failed to load brush preset {}", brush.id());
                        return;
                    };
                    let instance = Arc::new(RwLock::new(instance));

                    editor.selected = Some(Selected::Brush(SelectedBrush {
                        asset_id: Some(brush.id()),
                        instance: instance.clone(),
                        viewing_graph: BrushPresetGraph::Main,
                    }));

                    let device = cx.global::<RenderContext>().device.clone();
                    let queue = cx.global::<RenderContext>().queue.clone();
                    cx.set_global(CurrentBrushPresetOperator::new(BrushPresetOperator::new(
                        instance,
                        device,
                        queue,
                        InputProcessor::default(),
                    )));
                }
                ListEvent::Cancel => todo!(),
            },
        )
        .detach();

        cx.subscribe_in(
            &functions,
            window,
            |editor, functions_entity, event: &ListEvent, window, cx| match event {
                ListEvent::Select(_) => todo!(),
                ListEvent::Confirm(ix) => {
                    let Some(func) = functions_entity.update(cx, |funcs, cx| {
                        let item = funcs.delegate().get(*ix)?;
                        Some(item.handle.clone())
                    }) else {
                        return;
                    };

                    let ser_func = match func.get() {
                        Ok(ser_func) => ser_func,
                        Err(err) => {
                            log::error!("Failed reading function {}: {:?}", func.id(), err);
                            return;
                        }
                    };
                    let (maybe_func, errs) = ser_func.deserialize_func(
                        Some(func.id()),
                        FUNCTION_GRAPH_TYPE_REGISTRY.clone(),
                        FUNCTION_GRAPH_NODE_REGISTRY.as_ref(),
                    );

                    if !errs.is_empty() {
                        for err in errs {
                            log::error!("Error deserializing function {:?}: {:?}", func.id(), err);
                        }
                        return;
                    }
                    let Some(func) = maybe_func else {
                        return;
                    };

                    editor.selected = Some(Selected::Function(SelectedFunction {
                        asset_id: func.asset_id,
                        id: func.id,
                        instance: GraphFunctionInstance::new(func),
                    }));
                }
                ListEvent::Cancel => todo!(),
            },
        )
        .detach();

        cx.subscribe_in(
            &name_input_state,
            window,
            |editor, input_state, event: &InputEvent, window, cx| match event {
                InputEvent::PressEnter { secondary } => {
                    if let Some(selected) = &mut editor.selected {
                        let name = input_state.read(cx).value();
                        selected.set_name(name.into());
                    }
                }
                InputEvent::Blur => {
                    if let Some(selected) = &editor.selected {
                        input_state
                            .update(cx, |state, cx| state.set_value(selected.name(), window, cx));
                    }
                }
                InputEvent::Change | InputEvent::Focus => {}
            },
        )
        .detach();

        let new_ext_var_name_input_state = cx.new(|cx| InputState::new(window, cx));
        let new_ext_var_type_select_state = cx.new(|cx| {
            SelectState::new(
                BRUSH_GRAPH_TYPES
                    .all_types()
                    .keys()
                    .cloned()
                    .collect::<Vec<_>>(),
                None,
                window,
                cx,
            )
        });

        Self {
            selected: None,
            texture_storage,
            main_function_storage: function_storage,
            stroke_pp_function_storage: Arc::new(GraphFunctionStorage::new(HashMap::new())), // TODO

            saved_runtime_revision: 0,
            editor_state,
            brushes,
            functions,
            name_input_state,
            new_ext_var_name_input_state,
            new_ext_var_type_select_state,
            pane_selection: PaneSelection::Brush,
        }
    }

    pub fn on_save_current_item_action(
        &mut self,
        _: &SaveCurrentItem,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(selected) = &mut self.selected else {
            return;
        };

        let assets = cx.assets();

        match selected {
            Selected::Brush(brush) => {
                let instance = brush.instance.read();
                self.saved_runtime_revision = instance.runtime_revision();
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
                                Uuid::from_str("b92c20f6-8cdb-42b8-efae-a92705efd029").unwrap(),
                            ),
                            format!("{}.cbp", instance.metadata().name),
                            Arc::new(preset),
                        )
                        .unwrap();
                    brush.asset_id = Some(new_id);
                }

                let device = cx.global::<RenderContext>().device.clone();
                let queue = cx.global::<RenderContext>().queue.clone();
                cx.set_global(CurrentBrushPresetOperator::new(BrushPresetOperator::new(
                    brush.instance.clone(),
                    device,
                    queue,
                    InputProcessor::default(),
                )));
            }
            Selected::Function(func) => {
                let ser_func =
                    SerializableGraphFunction::serialize_func(func.instance.graph_function())
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
                                Uuid::from_str("b92c20f6-8cdb-42b8-efae-a92705efd029").unwrap(),
                            ),
                            format!("{}.csf", func.instance.graph_function().name),
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

impl Render for BrushEditor {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let left_pane = v_flex()
            .w(relative(0.2))
            .min_w(px(250.0))
            .max_h(px(400.0))
            .child(
                h_flex()
                    .justify_between()
                    .child(Button::new("brushes-button").on_click({
                        let editor = cx.entity().downgrade();

                        move |_, window, cx| {
                            editor.update(cx, |editor, cx| {
                                let id = GraphFunctionId::new(Uuid::new_v4());
                                editor.selected = Some(Selected::Function(SelectedFunction {
                                    asset_id: None,
                                    id,
                                    instance: GraphFunctionInstance::new(GraphFunction {
                                        asset_id: None,
                                        id,
                                        name: "[Unnamed Function]".to_string(),
                                        graph: Graph::new(
                                            GraphResources {
                                                functions: editor.main_function_storage.clone(),
                                                ..Default::default()
                                            },
                                            FUNCTION_GRAPH_TYPE_REGISTRY.clone(),
                                        ),
                                    }),
                                }));
                                editor.saved_runtime_revision = 0;
                            });
                        }
                    }))
                    .child(Button::new("functions-button").on_click({
                        let editor = cx.entity().downgrade();

                        move |_, window, cx| {
                            editor.update(cx, |editor, cx| {
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
                                let assets = cx.assets();
                                let id = assets
                                    .add_asset(
                                        // TODO
                                        BundleId::new(
                                            Uuid::from_str("b92c20f6-8cdb-42b8-efae-a92705efd029")
                                                .unwrap(),
                                        ),
                                        "unnamed_brush.cbp".to_string(),
                                        new_brush.clone(),
                                    )
                                    .unwrap();
                                let handle = assets.handle(id).unwrap();

                                let (instance, _) = BrushPresetInstance::from_asset(
                                    &handle,
                                    editor.texture_storage.clone(),
                                    editor.main_function_storage.clone(),
                                    editor.stroke_pp_function_storage.clone(),
                                );
                                editor.selected = Some(Selected::Brush(SelectedBrush {
                                    asset_id: None,
                                    instance: Arc::new(RwLock::new(instance.unwrap())),
                                    viewing_graph: BrushPresetGraph::Main,
                                }));
                                editor.saved_runtime_revision = 0;
                            });
                        }
                    })),
            )
            .child(v_flex().child(Button::new("new-item-button")).child(
                match self.pane_selection {
                    PaneSelection::Brush => List::new(&self.brushes).into_any_element(),
                    PaneSelection::Function => List::new(&self.functions).into_any_element(),
                },
            ));

        let editor = if let Some(selected) = &self.selected {
            let parent = cx.entity().downgrade();
            let title_widget = Input::new(&self.name_input_state);

            match selected {
                Selected::Brush(brush) => {
                    let instance = brush.instance.write();

                    let graph_view = match brush.viewing_graph {
                        BrushPresetGraph::RequiredSpacing => {
                            let edits = GraphEditSink::new(move |edit, cx| {
                                parent.update(cx, |editor, cx| {
                                    let Some(Selected::Brush(brush)) = &editor.selected else {
                                        return;
                                    };
                                    let mut instance = brush.instance.write();
                                    edit.apply(instance.required_spacing_graph_mut());
                                });
                            });
                            self.editor_state.update(cx, |editor, cx| {
                                editor.render_graph(
                                    instance.required_spacing_graph(),
                                    REQUIRED_SPACING_GRAPH_NODES.as_ref(),
                                    edits,
                                    window,
                                    cx,
                                )
                            })
                        }
                        BrushPresetGraph::Main => {
                            let edits = GraphEditSink::new(move |edit, cx| {
                                parent.update(cx, |editor, cx| {
                                    let Some(Selected::Brush(brush)) = &editor.selected else {
                                        return;
                                    };
                                    let mut instance = brush.instance.write();
                                    edit.apply(instance.main_graph_mut());
                                });
                            });
                            self.editor_state.update(cx, |editor, cx| {
                                editor.render_graph(
                                    instance.main_graph(),
                                    MAIN_GRAPH_NODES.as_ref(),
                                    edits,
                                    window,
                                    cx,
                                )
                            })
                        }
                        BrushPresetGraph::StrokePostprocess { index } => {
                            let edits = GraphEditSink::new(move |edit, cx| {
                                parent.update(cx, |editor, cx| {
                                    let Some(Selected::Brush(brush)) = &editor.selected else {
                                        return;
                                    };
                                    let mut instance = brush.instance.write();
                                    edit.apply(instance.stroke_postprocess_graph_mut(index));
                                });
                            });
                            self.editor_state.update(cx, |editor, cx| {
                                editor.render_graph(
                                    instance.stroke_postprocess_graph(index),
                                    STROKE_POSTPROCESS_GRAPH_NODES.as_ref(),
                                    edits,
                                    window,
                                    cx,
                                )
                            })
                        }
                    };

                    let external_vars = v_flex()
                        .child(
                            v_flex().children(instance.iter_external_vars().map(|(id, var)| {
                                h_flex()
                                    .child(
                                        v_flex().child(
                                            h_flex()
                                                .child(
                                                    Button::new(format!(
                                                        "remove-external-variable-{}-button",
                                                        id
                                                    ))
                                                    .on_click({
                                                        let editor = cx.entity().downgrade();
                                                        move |_, window, cx| {
                                                            editor.update(cx, |editor, cx| {
                                                                let Some(Selected::Brush(brush)) =
                                                                    editor.selected.as_ref()
                                                                else {
                                                                    return;
                                                                };

                                                                let mut instance =
                                                                    brush.instance.write();
                                                                instance.remove_external_var(&id);
                                                            });
                                                        }
                                                    })
                                                    .child(var.name),
                                                )
                                                .child(var.value.ty().name()),
                                        ),
                                    )
                                    .child(var.value.ty().render_inline(
                                        var.value.value(),
                                        GraphInlineLiteralRenderContext {
                                            slot_id: (*id).into(),
                                            window,
                                            cx,
                                            on_update: Rc::new(|value, cx| todo!()),
                                        },
                                    ))
                            })),
                        )
                        .child(
                            v_form()
                                .layout(Axis::Horizontal)
                                .child(
                                    field()
                                        .label("Name")
                                        .child(Input::new(&self.new_ext_var_name_input_state)),
                                )
                                .child(
                                    field()
                                        .label("Type")
                                        .child(Select::new(&self.new_ext_var_type_select_state)),
                                ),
                        )
                        .child(Button::new("create-external-variable-button").on_click({
                            let editor = cx.entity().downgrade();
                            move |_, window, cx| {
                                editor.update(cx, |editor, cx| {
                                    let Some(Selected::Brush(brush)) = editor.selected.as_ref()
                                    else {
                                        return;
                                    };

                                    let name = editor.new_ext_var_name_input_state.update(
                                        cx,
                                        |state, cx| {
                                            let name = state.value();
                                            state.set_value("", window, cx);
                                            name
                                        },
                                    );
                                    let mut instance = brush.instance.write();

                                    let Some(ty) = editor
                                        .new_ext_var_type_select_state
                                        .read(cx)
                                        .selected_value()
                                        .and_then(|ty_name| {
                                            instance.main_graph().type_registry().get_type(*ty_name)
                                        })
                                    else {
                                        return;
                                    };

                                    let id = ExternalVariableId::new(Uuid::new_v4());
                                    let value =
                                        GraphLiteral::new_boxed(ty.default_literal(), ty.clone());
                                    instance.insert_external_var(ExternalVariable {
                                        id,
                                        name: name.into(),
                                        value,
                                    });
                                });
                            }
                        }));

                    let graph_switcher = h_flex()
                        .child(
                            Button::new("select-required-spacing-button")
                                .label("Required Spacing")
                                .on_click({
                                    let editor = cx.entity().downgrade();
                                    move |_, window, cx| {
                                        editor.update(cx, |editor, cx| {
                                            if let Some(Selected::Brush(brush)) =
                                                &mut editor.selected
                                            {
                                                brush.viewing_graph =
                                                    BrushPresetGraph::RequiredSpacing;
                                            }
                                        });
                                    }
                                }),
                        )
                        .child(
                            Button::new("select-main-graph-button")
                                .label("Main")
                                .on_click({
                                    let editor = cx.entity().downgrade();
                                    move |_, window, cx| {
                                        editor.update(cx, |editor, cx| {
                                            if let Some(Selected::Brush(brush)) =
                                                &mut editor.selected
                                            {
                                                brush.viewing_graph = BrushPresetGraph::Main;
                                            }
                                        });
                                    }
                                }),
                        )
                        .children(
                            (0..instance.stroke_postprocess_graphs().len()).map(|index| {
                                Button::new(format!("select-stroke-pp-graph-button-{}", index))
                                    .on_click({
                                        let editor = cx.entity().downgrade();
                                        move |_, window, cx| {
                                            editor.update(cx, |editor, cx| {
                                                if let Some(Selected::Brush(brush)) =
                                                    &mut editor.selected
                                                {
                                                    brush.viewing_graph =
                                                        BrushPresetGraph::StrokePostprocess {
                                                            index,
                                                        };
                                                }
                                            });
                                        }
                                    })
                            }),
                        )
                        .child(Button::new("new-stroke-pp-graph-button").on_click({
                            let editor = cx.entity().downgrade();
                            move |_, window, cx| {
                                editor.update(cx, |editor, cx| {
                                    let Some(Selected::Brush(brush)) = &mut editor.selected else {
                                        return;
                                    };

                                    let index =
                                        brush.instance.write().new_stroke_postprocess_graph();
                                    brush.viewing_graph =
                                        BrushPresetGraph::StrokePostprocess { index };
                                });
                            }
                        }));

                    h_flex()
                        .child(v_flex().child(title_widget).child(graph_view))
                        .child(graph_switcher)
                        .child(external_vars)
                        .into_any_element()
                }
                Selected::Function(func) => {
                    let edits = GraphEditSink::new(move |edit, cx| {
                        parent.update(cx, |parent, cx| {
                            if let Some(Selected::Function(func)) = parent.selected.as_mut() {
                                edit.apply(&mut func.instance.graph_function_mut().graph);
                            }
                        });
                    });

                    let editor = self.editor_state.update(cx, |editor, cx| {
                        editor.render_graph(
                            &func.instance.graph_function().graph,
                            FUNCTION_GRAPH_NODE_REGISTRY.as_ref(),
                            edits,
                            window,
                            cx,
                        )
                    });

                    v_flex()
                        .child(title_widget)
                        .child(editor)
                        .into_any_element()
                }
            }
        } else {
            "No item selected".into_any_element()
        };

        h_flex().child(left_pane).child(editor)
    }
}

pub enum PaneSelection {
    Brush,
    Function,
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
