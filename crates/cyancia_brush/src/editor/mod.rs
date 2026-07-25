use std::{
    collections::HashMap,
    rc::Rc,
    sync::{Arc, LazyLock},
};

use cyancia_assets::{AssetAppExt, asset::AssetId};
use cyancia_render::texture::Image;
use cyancia_shader_graph::{
    editor::GraphEditor,
    graph::{
        Graph, GraphResources,
        external::{ExternalVariable, ExternalVariableId},
        function::{GraphFunction, GraphFunctionId, GraphFunctionStorage},
        node::GraphNodeRegistry,
        slot::GraphInlineLiteralRenderContext,
        texture::GraphTextureStorage,
        variable::{GraphLiteral, GraphTypeRegistry},
    },
    save::{SerializableGraph, SerializableGraphFunction},
    wgsl_std::{
        builtin_nodes, builtin_types,
        nodes::{GraphInputNode, GraphOutputNode},
    },
};
use gpui::{
    Action, App, AppContext, Axis, BorrowAppContext, ClickEvent, Context, Entity,
    InteractiveElement, IntoElement, KeyBinding, ParentElement, Render, Styled, Window, actions,
    div, px,
};
use gpui_component::{
    IconName, Selectable,
    button::{Button, ButtonGroup},
    form::{field, v_form},
    h_flex,
    input::{Input, InputEvent, InputState},
    list::{List, ListEvent, ListState},
    menu::{ContextMenuExt, DropdownMenu, PopupMenu, PopupMenuItem},
    scroll::ScrollableElement,
    select::{Select, SelectState},
    v_flex,
};
use uuid::Uuid;

use crate::{
    asset::{BrushPreset, BrushPresetMetadata},
    instance::{
        BRUSH_GRAPH_TYPES, BrushPresetInstance, GraphFunctionInstance, MAIN_GRAPH_NODES,
        REQUIRED_SPACING_GRAPH_NODES, STROKE_POSTPROCESS_GRAPH_NODES,
    },
    render::graph::{BrushGraphData, BrushGraphPostprocessData},
    tool::CurrentBrushPresetHandle,
    widget::{BrushFunctionListDelegate, BrushPresetListDelegate},
};

pub static FUNCTION_GRAPH_NODE_REGISTRY: LazyLock<Arc<GraphNodeRegistry<BrushGraphData>>> =
    LazyLock::new(|| {
        let mut registry = GraphNodeRegistry::default();

        registry.merge(builtin_nodes());
        registry.register::<GraphInputNode>();
        registry.register::<GraphOutputNode>();

        registry.into()
    });

pub static FUNCTION_GRAPH_TYPE_REGISTRY: LazyLock<Arc<GraphTypeRegistry>> = LazyLock::new(|| {
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

#[derive(Action, Clone, PartialEq, Eq)]
#[action(namespace = brush_editor, no_json)]
struct DeleteExternalVariable {
    id: ExternalVariableId,
}

// TODO: Tag filtering.
pub struct BrushEditor {
    texture_storage: Arc<GraphTextureStorage>,
    main_function_storage: Arc<GraphFunctionStorage<BrushGraphData>>,
    stroke_pp_function_storage: Arc<GraphFunctionStorage<BrushGraphPostprocessData>>,

    selected: Option<Selected>,

    editor_state: Option<EditorState>,
    brushes: Entity<ListState<BrushPresetListDelegate>>,
    functions: Entity<ListState<BrushFunctionListDelegate>>,
    name_input_state: Entity<InputState>,
    new_ext_var_name_input_state: Entity<InputState>,
    new_ext_var_type_select_state: Entity<SelectState<Vec<&'static str>>>,
    rename_ext_var_input_state: Entity<InputState>,
    renaming_ext_var: Option<ExternalVariableId>,
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
                        cx,
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

        let brushes =
            cx.new(|cx| ListState::new(BrushPresetListDelegate::new(brush_assets), window, cx));
        let functions = cx
            .new(|cx| ListState::new(BrushFunctionListDelegate::new(function_assets), window, cx));
        let name_input_state = cx.new(|cx| InputState::new(window, cx));

        cx.subscribe_in(&brushes, window, Self::on_brush_list_event)
            .detach();

        cx.subscribe_in(&functions, window, Self::on_function_list_event)
            .detach();

        cx.subscribe_in(&name_input_state, window, Self::on_name_input_event)
            .detach();

        let new_ext_var_name_input_state = cx.new(|cx| InputState::new(window, cx));
        let rename_ext_var_input_state = cx.new(|cx| InputState::new(window, cx));
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

        cx.subscribe_in(
            &rename_ext_var_input_state,
            window,
            Self::on_ext_var_input_event,
        )
        .detach();

        cx.observe_window_activation(window, Self::on_window_activation_changed)
            .detach();

        Self {
            selected: None,
            texture_storage,
            main_function_storage: function_storage,
            stroke_pp_function_storage: Arc::new(GraphFunctionStorage::new(HashMap::new())), // TODO

            editor_state: None,
            brushes,
            functions,
            name_input_state,
            new_ext_var_name_input_state,
            new_ext_var_type_select_state,
            rename_ext_var_input_state,
            renaming_ext_var: None,
            pane_selection: PaneSelection::Brush,
        }
    }

    fn on_brush_list_event(
        &mut self,
        brushes_entity: &Entity<ListState<BrushPresetListDelegate>>,
        event: &ListEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            ListEvent::Select(_) => {}
            ListEvent::Confirm(ix) => {
                self.functions.update(cx, |funcs, cx| {
                    funcs.set_selected_index(None, window, cx);
                });

                let Some(brush) = brushes_entity.update(cx, |brushes, _| {
                    let item = brushes.delegate().get(*ix)?;
                    Some(item.handle.clone())
                }) else {
                    return;
                };

                let (maybe_instance, errs) = BrushPresetInstance::from_asset(
                    &brush,
                    self.texture_storage.clone(),
                    self.main_function_storage.clone(),
                    self.stroke_pp_function_storage.clone(),
                    cx,
                );

                for err in errs {
                    log::error!("Error deserializing brush preset {}: {}", brush.id(), err);
                }

                let Some(instance) = maybe_instance else {
                    log::error!("Failed to load brush preset {}", brush.id());
                    return;
                };

                self.name_input_state.update(cx, |st, cx| {
                    st.set_value(instance.metadata().name.clone(), window, cx);
                });
                self.editor_state = Some(EditorState::Main(cx.new(|cx| {
                    GraphEditor::new(instance.main_graph().clone(), MAIN_GRAPH_NODES.clone(), cx)
                })));
                self.selected = Some(Selected::Brush(SelectedBrush {
                    asset_id: brush.id(),
                    instance,
                    viewing_graph: BrushPresetGraph::Main,
                }));
            }
            ListEvent::Cancel => {}
        }
    }

    fn on_function_list_event(
        &mut self,
        functions_entity: &Entity<ListState<BrushFunctionListDelegate>>,
        event: &ListEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            ListEvent::Select(_) => {}
            ListEvent::Confirm(ix) => {
                self.brushes.update(cx, |brushes, cx| {
                    brushes.set_selected_index(None, window, cx);
                });

                let Some(func) = functions_entity.update(cx, |funcs, _| {
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
                    cx,
                );

                for err in errs {
                    log::error!("Error deserializing function {:?}: {:?}", func.id(), err);
                }
                let Some(func) = maybe_func else {
                    log::error!("Failed to load function {}", func.id());
                    return;
                };

                self.name_input_state.update(cx, |st, cx| {
                    st.set_value(func.name.clone(), window, cx);
                });
                self.editor_state = Some(EditorState::Main(cx.new(|cx| {
                    GraphEditor::new(func.graph.clone(), FUNCTION_GRAPH_NODE_REGISTRY.clone(), cx)
                })));
                self.selected = Some(Selected::Function(SelectedFunction {
                    asset_id: func.asset_id.unwrap(),
                    id: func.id,
                    instance: GraphFunctionInstance::new(func),
                }));
            }
            ListEvent::Cancel => {}
        }
    }

    fn on_name_input_event(
        &mut self,
        input_state: &Entity<InputState>,
        event: &InputEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            InputEvent::PressEnter { secondary: _ } => {
                if let Some(selected) = &mut self.selected {
                    let name = input_state.read(cx).value();
                    match selected {
                        Selected::Brush(brush) => {
                            brush.instance.metadata_mut().name = name.into();
                        }
                        Selected::Function(func) => {
                            func.instance.graph_function_mut().name = name.into()
                        }
                    }
                }
            }
            InputEvent::Blur => {
                if let Some(selected) = &self.selected {
                    let name = match selected {
                        Selected::Brush(brush) => brush.instance.metadata().name.clone(),
                        Selected::Function(func) => func.instance.graph_function().name.clone(),
                    };
                    input_state.update(cx, |state, cx| state.set_value(name, window, cx));
                }
            }
            InputEvent::Change | InputEvent::Focus => {}
        }
    }

    fn on_ext_var_input_event(
        &mut self,
        _: &Entity<InputState>,
        event: &InputEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            InputEvent::PressEnter { secondary: _ } => {
                if let Some(id) = self.renaming_ext_var {
                    self.confirm_external_var_rename(id, cx);
                }
            }
            InputEvent::Blur | InputEvent::Change | InputEvent::Focus => {}
        }
    }

    fn confirm_external_var_rename(&mut self, id: ExternalVariableId, cx: &mut Context<Self>) {
        self.renaming_ext_var = None;
        let name = self.rename_ext_var_input_state.read(cx).value();
        if name.is_empty() {
            return;
        }

        let Some(Selected::Brush(brush)) = &mut self.selected else {
            return;
        };
        brush.instance.rename_external_var(&id, name.into());
        cx.notify();
    }

    fn on_window_activation_changed(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if window.is_window_active() {
            return;
        }

        let Some(selected) = &self.selected else {
            return;
        };

        match selected {
            Selected::Brush(brush) => {
                let preset = brush.instance.as_asset(cx).unwrap();
                let assets = cx.assets();

                let handle = assets.handle(brush.asset_id).unwrap();
                handle.update(preset).unwrap();

                if let Some(cur_handle) = cx.try_global::<CurrentBrushPresetHandle>()
                    && cur_handle.0.id() == handle.id()
                {
                    // To notify the tool
                    cx.set_global(CurrentBrushPresetHandle(handle));
                }
            }
            Selected::Function(func) => {
                let assets = cx.assets();
                let ser_func =
                    SerializableGraphFunction::serialize_func(func.instance.graph_function(), cx)
                        .unwrap();
                let handle = assets.handle(func.asset_id).unwrap();
                handle.update(ser_func).unwrap();

                // We still needs to notify the tool. As user adjusting the function, the current brush is likely
                // referencing the function. So we are refreshing the global brush.
                if let Some(cur_handle) = cx.try_global::<CurrentBrushPresetHandle>() {
                    let global = cur_handle.clone();
                    // To notify the tool
                    cx.set_global(global);
                }
            }
        }
    }

    pub fn on_save_current_item_action(
        &mut self,
        _: &SaveCurrentItem,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(selected) = &mut self.selected else {
            return;
        };
        let assets = cx.assets();

        match selected {
            Selected::Brush(brush) => {
                let preset = brush.instance.as_asset(cx).unwrap();

                let handle = assets.handle(brush.asset_id).unwrap();
                handle.update(preset).unwrap();
                handle.write().unwrap();
            }
            Selected::Function(func) => {
                let ser_func =
                    SerializableGraphFunction::serialize_func(func.instance.graph_function(), cx)
                        .unwrap();
                let handle = assets.handle(func.asset_id).unwrap();
                handle.update(ser_func).unwrap();
                handle.write().unwrap();
            }
        }

        log::info!("Saved current item.")
    }

    fn on_new_item(&mut self, _: &ClickEvent, window: &mut Window, cx: &mut Context<Self>) {
        let assets = cx.assets();
        let Some(bundle) = assets
            .bundles()
            .find(|b| !b.is_readonly())
            .map(|b| b.metadata().bundle_id)
        else {
            return;
        };

        match self.pane_selection {
            PaneSelection::Brush => {
                let new_brush = BrushPreset {
                    metadata: BrushPresetMetadata {
                        name: "[Unnamed Brush]".to_string(),
                    },
                    required_spacing_graph: SerializableGraph::default(),
                    main_graph: SerializableGraph::default(),
                    stroke_postprocess_graphs: Vec::new(),
                    external_vars: Vec::new(),
                };

                let (instance, _) = BrushPresetInstance::new(
                    &new_brush,
                    self.texture_storage.clone(),
                    self.main_function_storage.clone(),
                    self.stroke_pp_function_storage.clone(),
                    cx,
                );
                let Some(instance) = instance else {
                    return;
                };

                let new_id = cx
                    .assets()
                    .add_asset(
                        bundle,
                        format!("{}.cbp", instance.metadata().name),
                        Arc::new(instance.as_asset(cx).unwrap()),
                    )
                    .unwrap();

                self.name_input_state.update(cx, |state, cx| {
                    state.set_value(instance.metadata().name.clone(), window, cx);
                });
                self.editor_state = Some(EditorState::new_main(instance.main_graph().clone(), cx));
                self.selected = Some(Selected::Brush(SelectedBrush {
                    asset_id: new_id,
                    instance,
                    viewing_graph: BrushPresetGraph::Main,
                }));
            }
            PaneSelection::Function => {
                let id = GraphFunctionId::new(Uuid::new_v4());
                let instance = GraphFunctionInstance::new(GraphFunction {
                    asset_id: None,
                    id,
                    name: "[Unnamed Function]".to_string(),
                    graph: cx.new(|_| {
                        Graph::new(
                            GraphResources {
                                functions: self.main_function_storage.clone(),
                                ..Default::default()
                            },
                            FUNCTION_GRAPH_TYPE_REGISTRY.clone(),
                        )
                    }),
                });

                let ser_func =
                    SerializableGraphFunction::serialize_func(instance.graph_function(), cx)
                        .unwrap();
                let new_id = cx
                    .assets()
                    .add_asset(
                        bundle,
                        format!("{}.csf", instance.graph_function().name),
                        Arc::new(ser_func),
                    )
                    .unwrap();

                self.name_input_state.update(cx, |state, cx| {
                    state.set_value(instance.graph_function().name.clone(), window, cx)
                });
                self.editor_state = Some(EditorState::new_function(
                    instance.graph_function().graph.clone(),
                    cx,
                ));

                self.selected = Some(Selected::Function(SelectedFunction {
                    asset_id: new_id,
                    id,
                    instance,
                }));
            }
        }
    }

    fn on_new_external_variable(
        &mut self,
        _: &ClickEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(Selected::Brush(brush)) = &mut self.selected else {
            return;
        };

        let Some(ty) = self
            .new_ext_var_type_select_state
            .read(cx)
            .selected_value()
            .and_then(|ty_name| {
                brush
                    .instance
                    .main_graph()
                    .read(cx)
                    .type_registry()
                    .get_type(ty_name)
            })
        else {
            return;
        };

        let id = ExternalVariableId::new(Uuid::new_v4());
        let value = GraphLiteral::new_boxed(ty.default_literal(), dyn_clone::clone_box(ty));

        let Some(name) = self.new_ext_var_name_input_state.update(cx, |state, cx| {
            let name = state.value();
            if name.is_empty() {
                return None;
            }
            state.set_value("", window, cx);
            Some(name)
        }) else {
            return;
        };

        brush.instance.insert_external_var(ExternalVariable {
            id,
            name: name.into(),
            value,
        });
    }

    fn render_brush_extra_panes(
        &self,
        instance: &BrushPresetInstance,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let editor = cx.entity().downgrade();
        let ext_vars = instance.iter_external_vars().map(|(id, var)| {
            let is_renaming = self.renaming_ext_var == Some(id);
            let name = var.name.clone();
            let name_row = if is_renaming {
                h_flex()
                    .gap_1()
                    .child(Input::new(&self.rename_ext_var_input_state).flex_1())
                    .child(
                        Button::new(format!("confirm-external-variable-rename-button-{}", id))
                            .icon(IconName::Check)
                            .on_click(cx.listener(move |editor, _, _, cx| {
                                editor.confirm_external_var_rename(id, cx);
                            })),
                    )
                    .child(
                        Button::new(format!("cancel-external-variable-rename-button-{}", id))
                            .icon(IconName::Close)
                            .on_click(cx.listener(move |editor, _, _, _| {
                                editor.renaming_ext_var = None;
                            })),
                    )
                    .into_any_element()
            } else {
                let drop_down = {
                    let editor = cx.entity().downgrade();
                    let name = name.clone();
                    move |menu: PopupMenu, _: &mut Window, _: &mut Context<PopupMenu>| {
                        menu.item(PopupMenuItem::new("Rename").on_click({
                            let editor = editor.clone();
                            let name = name.clone();
                            move |_, window, cx| {
                                let _ = editor.update(cx, |editor, cx| {
                                    editor.renaming_ext_var = Some(id);
                                    editor.rename_ext_var_input_state.update(cx, |state, cx| {
                                        state.set_value(name.clone(), window, cx);
                                    });
                                    cx.notify();
                                });
                            }
                        }))
                        .item(PopupMenuItem::new("Remove").on_click({
                            let editor = editor.clone();
                            move |_, _, cx| {
                                editor
                                    .update(cx, |editor, cx| {
                                        let Some(Selected::Brush(brush)) = &mut editor.selected
                                        else {
                                            return;
                                        };
                                        brush.instance.remove_external_var(&id);
                                        cx.notify();
                                    })
                                    .ok();
                            }
                        }))
                    }
                };

                h_flex()
                    .justify_between()
                    .child(name.clone())
                    .child(
                        Button::new(format!("external-variable-menu-{}", id))
                            .icon(IconName::Menu)
                            .dropdown_menu(drop_down),
                    )
                    .into_any_element()
            };

            v_flex()
                .gap_1()
                .child(
                    v_flex().child(name_row).child(
                        div()
                            .opacity(0.7)
                            .italic()
                            .text_sm()
                            .child(var.value.ty().name()),
                    ),
                )
                .child(var.value.ty().render_inline(
                    var.value.value(),
                    GraphInlineLiteralRenderContext {
                        slot_id: (*id).into(),
                        window,
                        cx,
                        on_update: Rc::new({
                            let editor = editor.clone();
                            move |value, cx| {
                                let _ = editor.update(cx, |editor, cx| {
                                    let Some(Selected::Brush(brush)) = &mut editor.selected else {
                                        return;
                                    };
                                    brush.instance.update_external_var(&id, value);
                                    cx.notify();
                                });
                            }
                        }),
                    },
                ))
        });
        let ext_var_panel = v_flex()
            .w(px(200.0))
            .overflow_hidden()
            .child(div().flex_shrink_0().child("External variables"))
            .child(
                div().flex_1().overflow_hidden().child(
                    v_flex()
                        .size_full()
                        .gap_1()
                        .children(ext_vars)
                        .overflow_y_scrollbar(),
                ),
            )
            .child(
                v_flex()
                    .gap_1()
                    .child(
                        v_form()
                            .label_width(px(70.0))
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
                    .child(
                        Button::new("create-external-variable-button")
                            .label("Create")
                            .on_click(cx.listener(Self::on_new_external_variable)),
                    ),
            );

        let graph_switcher = v_flex()
            .w(px(200.0))
            .child(
                Button::new("select-required-spacing-button")
                    .label("Required Spacing")
                    .on_click(cx.listener(Self::on_select_required_spacing_graph)),
            )
            .child(
                Button::new("select-main-graph-button")
                    .label("Main")
                    .on_click(cx.listener(Self::on_select_main_graph)),
            )
            .children(
                (0..instance.stroke_postprocess_graphs().len()).map(|index| {
                    let context_menu = {
                        let editor = cx.entity().downgrade();
                        move |menu: PopupMenu, _: &mut Window, _: &mut Context<PopupMenu>| {
                            menu.item(PopupMenuItem::new("Remove").on_click({
                                let editor = editor.upgrade().unwrap();
                                move |_, _, cx| {
                                    editor.update(cx, |editor, cx| {
                                        let Some(Selected::Brush(brush)) = &mut editor.selected
                                        else {
                                            return;
                                        };

                                        brush.instance.remove_stroke_postprocess_graph(index);
                                        if brush.viewing_graph
                                            == (BrushPresetGraph::StrokePostprocess { index })
                                        {
                                            let n_pp =
                                                brush.instance.stroke_postprocess_graphs().len();
                                            if n_pp == 0 {
                                                brush.viewing_graph = BrushPresetGraph::Main;
                                            } else {
                                                brush.viewing_graph =
                                                    BrushPresetGraph::StrokePostprocess {
                                                        index: n_pp - 1,
                                                    };
                                            }
                                        }
                                        cx.notify();
                                    });
                                }
                            }))
                        }
                    };

                    Button::new(format!("select-stroke-pp-graph-button-{}", index))
                        .label(format!("Stroke Postprocess {}", index))
                        .on_click(cx.listener(move |editor, event, window, cx| {
                            editor.on_select_stroke_pp_graph(index, event, window, cx);
                        }))
                        .context_menu(context_menu)
                }),
            )
            .child(
                Button::new("new-stroke-pp-graph-button")
                    .label("New Stroke Postprocess")
                    .on_click(cx.listener(|editor, _, _, cx| {
                        let Some(Selected::Brush(brush)) = &mut editor.selected else {
                            return;
                        };

                        brush.viewing_graph = BrushPresetGraph::StrokePostprocess {
                            index: brush.instance.new_stroke_postprocess_graph(cx),
                        };
                    })),
            );

        h_flex()
            .gap_2()
            .h_full()
            .child(graph_switcher.h_full())
            .child(ext_var_panel.h_full())
    }

    fn on_select_required_spacing_graph(
        &mut self,
        _: &ClickEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(Selected::Brush(brush)) = &mut self.selected else {
            return;
        };
        brush.viewing_graph = BrushPresetGraph::RequiredSpacing;
        let st = cx.new(|cx| {
            GraphEditor::new(
                brush.instance.required_spacing_graph().clone(),
                REQUIRED_SPACING_GRAPH_NODES.clone(),
                cx,
            )
        });
        self.editor_state = Some(EditorState::Main(st));
    }

    fn on_select_main_graph(&mut self, _: &ClickEvent, _: &mut Window, cx: &mut Context<Self>) {
        let Some(Selected::Brush(brush)) = &mut self.selected else {
            return;
        };
        brush.viewing_graph = BrushPresetGraph::Main;
        let st = cx.new(|cx| {
            GraphEditor::new(
                brush.instance.main_graph().clone(),
                MAIN_GRAPH_NODES.clone(),
                cx,
            )
        });
        self.editor_state = Some(EditorState::Main(st));
    }

    fn on_select_stroke_pp_graph(
        &mut self,
        index: usize,
        _: &ClickEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(Selected::Brush(brush)) = &mut self.selected else {
            return;
        };
        let Some(graph) = brush.instance.stroke_postprocess_graph(index) else {
            return;
        };
        brush.viewing_graph = BrushPresetGraph::StrokePostprocess { index };
        let st = cx
            .new(|cx| GraphEditor::new(graph.clone(), STROKE_POSTPROCESS_GRAPH_NODES.clone(), cx));
        self.editor_state = Some(EditorState::Postprocess(st));
    }
}

impl Render for BrushEditor {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let left_pane = v_flex()
            .w(px(256.0))
            .min_w(px(250.0))
            .max_w(px(320.0))
            .h_full()
            .child(
                ButtonGroup::new("items-button-group")
                    .child(
                        Button::new("brushes-button")
                            .label("Brushes")
                            .on_click(cx.listener(|editor, _, _, _| {
                                editor.pane_selection = PaneSelection::Brush;
                            }))
                            .selected(self.pane_selection == PaneSelection::Brush),
                    )
                    .child(
                        Button::new("functions-button")
                            .label("Functions")
                            .on_click(cx.listener(|editor, _, _, _| {
                                editor.pane_selection = PaneSelection::Function;
                            }))
                            .selected(self.pane_selection == PaneSelection::Function),
                    ),
            )
            .child(
                v_flex()
                    .flex_1()
                    .min_h(px(0.0))
                    .child(
                        Button::new("new-item-button")
                            .label(match self.pane_selection {
                                PaneSelection::Brush => "New Brush",
                                PaneSelection::Function => "New Function",
                            })
                            .on_click(cx.listener(Self::on_new_item)),
                    )
                    .child(match self.pane_selection {
                        PaneSelection::Brush => List::new(&self.brushes)
                            .w_full()
                            .flex_1()
                            .min_h(px(0.0))
                            .into_any_element(),
                        PaneSelection::Function => List::new(&self.functions)
                            .w_full()
                            .flex_1()
                            .min_h(px(0.0))
                            .into_any_element(),
                    }),
            );

        let editor = if let Some(selected) = &self.selected {
            let title_widget = h_flex()
                .gap_2()
                .child("Name")
                .child(Input::new(&self.name_input_state).w_full());
            let graph_view = match &self.editor_state {
                Some(EditorState::Main(e)) => e.clone().into_any_element(),
                Some(EditorState::Postprocess(e)) => e.clone().into_any_element(),
                Some(EditorState::Function(e)) => e.clone().into_any_element(),
                None => div().into_any_element(),
            };
            let common_editor = v_flex()
                .size_full()
                .min_w(px(0.0))
                .overflow_hidden()
                .gap_2()
                .child(title_widget)
                .child(
                    div()
                        .flex_1()
                        .min_h(px(0.0))
                        .overflow_hidden()
                        .child(graph_view),
                );

            match selected {
                Selected::Brush(brush) => h_flex()
                    .gap_2()
                    .size_full()
                    .child(common_editor)
                    .child(self.render_brush_extra_panes(&brush.instance, window, cx))
                    .into_any_element(),
                Selected::Function(_) => common_editor.into_any_element(),
            }
        } else {
            div()
                .size_full()
                .child("No item selected")
                .into_any_element()
        };

        h_flex()
            .key_context(BRUSH_EDITOR_CONTEXT)
            .on_action(cx.listener(Self::on_save_current_item_action))
            .p_1()
            .size_full()
            .overflow_hidden()
            .gap_2()
            .child(left_pane)
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .h_full()
                    .overflow_hidden()
                    .child(editor),
            )
    }
}

#[derive(Clone, PartialEq, Eq)]
pub enum PaneSelection {
    Brush,
    Function,
}

#[derive(Clone, PartialEq, Eq)]
pub enum BrushPresetGraph {
    RequiredSpacing,
    Main,
    StrokePostprocess { index: usize },
}

pub struct SelectedBrush {
    pub asset_id: AssetId<BrushPreset>,
    pub instance: BrushPresetInstance,
    pub viewing_graph: BrushPresetGraph,
}

pub struct SelectedFunction {
    pub asset_id: AssetId<SerializableGraphFunction>,
    pub id: GraphFunctionId,
    pub instance: GraphFunctionInstance,
}

pub enum Selected {
    Brush(SelectedBrush),
    Function(SelectedFunction),
}

pub enum EditorState {
    Main(Entity<GraphEditor<BrushGraphData>>),
    Postprocess(Entity<GraphEditor<BrushGraphPostprocessData>>),
    Function(Entity<GraphEditor<BrushGraphData>>),
}

impl EditorState {
    pub fn new_main(graph: Entity<Graph<BrushGraphData>>, cx: &mut App) -> Self {
        EditorState::Main(cx.new(|cx| GraphEditor::new(graph, MAIN_GRAPH_NODES.clone(), cx)))
    }

    pub fn new_postprocess(graph: Entity<Graph<BrushGraphPostprocessData>>, cx: &mut App) -> Self {
        EditorState::Postprocess(
            cx.new(|cx| GraphEditor::new(graph, STROKE_POSTPROCESS_GRAPH_NODES.clone(), cx)),
        )
    }

    pub fn new_function(graph: Entity<Graph<BrushGraphData>>, cx: &mut App) -> Self {
        EditorState::Function(
            cx.new(|cx| GraphEditor::new(graph, FUNCTION_GRAPH_NODE_REGISTRY.clone(), cx)),
        )
    }
}
