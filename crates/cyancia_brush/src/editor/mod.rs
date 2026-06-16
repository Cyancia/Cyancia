use std::{
    collections::HashMap,
    rc::Rc,
    str::FromStr,
    sync::{Arc, LazyLock},
};

use cyancia_assets::{AssetAppExt, asset::AssetId, bundle::BundleId};
use cyancia_render::{render_context::RenderContext, texture::Image};
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
    input_processing::InputProcessor,
    instance::{
        BRUSH_GRAPH_TYPES, BrushPresetInstance, GraphFunctionInstance, MAIN_GRAPH_NODES,
        REQUIRED_SPACING_GRAPH_NODES, STROKE_POSTPROCESS_GRAPH_NODES,
    },
    render::{
        BrushPresetOperator,
        graph::{BrushGraphData, BrushGraphPostprocessData},
    },
    tool::CurrentBrushPresetOperator,
    widget::{BrushFunctionListDelegate, BrushPresetListDelegate},
};

static FUNCTION_GRAPH_NODE_REGISTRY: LazyLock<Arc<GraphNodeRegistry<BrushGraphData>>> =
    LazyLock::new(|| {
        let mut registry = GraphNodeRegistry::default();

        registry.merge(builtin_nodes());
        registry.register::<GraphInputNode>();
        registry.register::<GraphOutputNode>();

        registry.into()
    });

static FUNCTION_GRAPH_TYPE_REGISTRY: LazyLock<Arc<GraphTypeRegistry>> = LazyLock::new(|| {
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

pub struct BrushEditor {
    texture_storage: Arc<GraphTextureStorage>,
    main_function_storage: Arc<GraphFunctionStorage<BrushGraphData>>,
    stroke_pp_function_storage: Arc<GraphFunctionStorage<BrushGraphPostprocessData>>,

    selected: Option<Selected>,

    saved_runtime_revision: u64,
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

        cx.subscribe_in(
            &brushes,
            window,
            move |editor, brushes_entity, event: &ListEvent, window, cx| match event {
                ListEvent::Select(_) => {}
                ListEvent::Confirm(ix) => {
                    editor.functions.update(cx, |funcs, cx| {
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
                        editor.texture_storage.clone(),
                        editor.main_function_storage.clone(),
                        editor.stroke_pp_function_storage.clone(),
                        cx,
                    );

                    for err in errs {
                        log::error!("Error deserializing brush preset {}: {}", brush.id(), err);
                    }

                    let Some(instance) = maybe_instance else {
                        log::error!("Failed to load brush preset {}", brush.id());
                        return;
                    };

                    editor.name_input_state.update(cx, |st, cx| {
                        st.set_value(instance.metadata().name.clone(), window, cx);
                    });

                    editor.selected = Some(Selected::Brush(SelectedBrush {
                        viewing_graph: BrushPresetGraph::Main,
                    }));
                    editor.editor_state = Some(EditorState::Main(cx.new(|cx| {
                        GraphEditor::new(
                            instance.main_graph().clone(),
                            MAIN_GRAPH_NODES.clone(),
                            cx,
                        )
                    })));

                    let device = cx.global::<RenderContext>().device.clone();
                    let queue = cx.global::<RenderContext>().queue.clone();
                    cx.set_global(CurrentBrushPresetOperator::new(Some(
                        BrushPresetOperator::new(
                            instance,
                            device,
                            queue,
                            InputProcessor::default(),
                        ),
                    )));
                }
                ListEvent::Cancel => {}
            },
        )
        .detach();

        cx.subscribe_in(
            &functions,
            window,
            move |editor, functions_entity, event: &ListEvent, window, cx| match event {
                ListEvent::Select(_) => {}
                ListEvent::Confirm(ix) => {
                    editor.brushes.update(cx, |brushes, cx| {
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
                    editor.name_input_state.update(cx, |st, cx| {
                        st.set_value(ser_func.name.clone(), window, cx);
                    });
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

                    editor.editor_state = Some(EditorState::Main(cx.new(|cx| {
                        GraphEditor::new(
                            func.graph.clone(),
                            FUNCTION_GRAPH_NODE_REGISTRY.clone(),
                            cx,
                        )
                    })));
                    editor.selected = Some(Selected::Function(SelectedFunction {
                        asset_id: func.asset_id,
                        id: func.id,
                        instance: GraphFunctionInstance::new(func),
                    }));
                }
                ListEvent::Cancel => {}
            },
        )
        .detach();

        cx.subscribe_in(
            &name_input_state,
            window,
            |editor, input_state, event: &InputEvent, window, cx| match event {
                InputEvent::PressEnter { secondary: _ } => {
                    if let Some(selected) = &mut editor.selected {
                        let name = input_state.read(cx).value();
                        match selected {
                            Selected::Brush(_) => {
                                if let Some(op) =
                                    cx.global_mut::<CurrentBrushPresetOperator>().as_mut()
                                {
                                    op.instance_mut().metadata_mut().name = name.into();
                                }
                            }
                            Selected::Function(func) => {
                                func.instance.graph_function_mut().name = name.into()
                            }
                        }
                    }
                }
                InputEvent::Blur => {
                    if let Some(selected) = &editor.selected {
                        let name = match selected {
                            Selected::Brush(_) => cx
                                .global::<CurrentBrushPresetOperator>()
                                .as_ref()
                                .map(|op| op.instance().metadata().name.clone())
                                .unwrap_or_default(),
                            Selected::Function(func) => func.instance.graph_function().name.clone(),
                        };
                        input_state.update(cx, |state, cx| state.set_value(name, window, cx));
                    }
                }
                InputEvent::Change | InputEvent::Focus => {}
            },
        )
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
            |editor, _, event: &InputEvent, _, cx| match event {
                InputEvent::PressEnter { secondary: _ } => {
                    if let Some(id) = editor.renaming_ext_var {
                        editor.confirm_external_var_rename(id, cx);
                    }
                }
                InputEvent::Blur | InputEvent::Change | InputEvent::Focus => {}
            },
        )
        .detach();

        Self {
            selected: None,
            texture_storage,
            main_function_storage: function_storage,
            stroke_pp_function_storage: Arc::new(GraphFunctionStorage::new(HashMap::new())), // TODO

            saved_runtime_revision: 0,
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

    fn confirm_external_var_rename(&mut self, id: ExternalVariableId, cx: &mut Context<Self>) {
        self.renaming_ext_var = None;
        let name = self.rename_ext_var_input_state.read(cx).value();
        if name.is_empty() {
            return;
        }

        let Some(Selected::Brush(_)) = self.selected.as_ref() else {
            return;
        };

        let Some(brush) = cx.global_mut::<CurrentBrushPresetOperator>().as_mut() else {
            return;
        };
        brush.instance_mut().rename_external_var(&id, name.into());
        cx.notify();
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

        match selected {
            Selected::Brush(_) => {
                let brush = cx.global::<CurrentBrushPresetOperator>();
                let Some(brush) = brush.as_ref() else {
                    return;
                };
                let brush = brush.instance();
                self.saved_runtime_revision = brush.runtime_revision();
                let preset = brush.as_asset(cx).unwrap();
                let assets = cx.assets();

                let handle = assets.handle(brush.asset_id()).unwrap();
                handle.update(preset).unwrap();
                handle.write().unwrap();
            }
            Selected::Function(func) => {
                let assets = cx.assets();
                let ser_func =
                    SerializableGraphFunction::serialize_func(func.instance.graph_function(), cx)
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

    fn on_new_item(&mut self, _: &ClickEvent, _: &mut Window, cx: &mut Context<Self>) {
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
                let new_brush = Arc::new(new_brush);
                let assets = cx.assets();
                let id = assets
                    .add_asset(
                        // TODO
                        BundleId::new(
                            Uuid::from_str("b92c20f6-8cdb-42b8-efae-a92705efd029").unwrap(),
                        ),
                        "unnamed_brush.cbp",
                        new_brush.clone(),
                    )
                    .unwrap();
                let handle = assets.handle(id).unwrap();

                let (instance, _) = BrushPresetInstance::from_asset(
                    &handle,
                    self.texture_storage.clone(),
                    self.main_function_storage.clone(),
                    self.stroke_pp_function_storage.clone(),
                    cx,
                );
                let Some(instance) = instance else {
                    return;
                };

                let render_context = cx.global::<RenderContext>();
                let op = BrushPresetOperator::new(
                    instance,
                    render_context.device.clone(),
                    render_context.queue.clone(),
                    Default::default(),
                );
                cx.set_global(CurrentBrushPresetOperator::new(Some(op)));
                self.selected = Some(Selected::Brush(SelectedBrush {
                    viewing_graph: BrushPresetGraph::Main,
                }));
                self.saved_runtime_revision = 0;
            }
            PaneSelection::Function => {
                let id = GraphFunctionId::new(Uuid::new_v4());
                self.selected = Some(Selected::Function(SelectedFunction {
                    asset_id: None,
                    id,
                    instance: GraphFunctionInstance::new(GraphFunction {
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
                    }),
                }));
                self.saved_runtime_revision = 0;
            }
        }
    }

    fn on_new_external_variable(
        &mut self,
        _: &ClickEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(Selected::Brush(_)) = self.selected.as_ref() else {
            return;
        };

        cx.update_global::<CurrentBrushPresetOperator, _>(|op, cx| {
            let Some(brush) = op.as_mut() else {
                return;
            };

            let Some(ty) = self
                .new_ext_var_type_select_state
                .read(cx)
                .selected_value()
                .and_then(|ty_name| {
                    brush
                        .instance()
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

            brush.instance_mut().insert_external_var(ExternalVariable {
                id,
                name: name.into(),
                value,
            });
        });
    }

    fn render_brush_extra_panes(
        &self,
        instance: &BrushPresetInstance,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
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
                                        let Some(Selected::Brush(_)) = editor.selected.as_ref()
                                        else {
                                            return;
                                        };
                                        let Some(brush) =
                                            cx.global_mut::<CurrentBrushPresetOperator>().as_mut()
                                        else {
                                            return;
                                        };

                                        brush.instance_mut().remove_external_var(&id);
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
                        on_update: Rc::new(move |value, cx| {
                            let Some(op) = cx.global_mut::<CurrentBrushPresetOperator>().as_mut()
                            else {
                                return;
                            };
                            op.instance_mut().update_external_var(&id, value);
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

                                        cx.update_global::<CurrentBrushPresetOperator, _>(
                                            |op, _| {
                                                let Some(op) = op.as_mut() else {
                                                    return;
                                                };
                                                op.instance_mut()
                                                    .remove_stroke_postprocess_graph(index);
                                                if brush.viewing_graph
                                                    == (BrushPresetGraph::StrokePostprocess {
                                                        index,
                                                    })
                                                {
                                                    let n_pp = op
                                                        .instance()
                                                        .stroke_postprocess_graphs()
                                                        .len();
                                                    if n_pp == 0 {
                                                        brush.viewing_graph =
                                                            BrushPresetGraph::Main;
                                                    } else {
                                                        brush.viewing_graph =
                                                            BrushPresetGraph::StrokePostprocess {
                                                                index: n_pp - 1,
                                                            };
                                                    }
                                                }
                                            },
                                        );
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

                        cx.update_global::<CurrentBrushPresetOperator, _>(|op, cx| {
                            let Some(op) = op.as_mut() else {
                                return;
                            };

                            brush.viewing_graph = BrushPresetGraph::StrokePostprocess {
                                index: op.instance_mut().new_stroke_postprocess_graph(cx),
                            };
                        });
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
        cx.update_global::<CurrentBrushPresetOperator, _>(|op, cx| {
            let Some(op) = op.as_ref() else {
                return;
            };

            if let Some(Selected::Brush(brush)) = &mut self.selected {
                brush.viewing_graph = BrushPresetGraph::RequiredSpacing;
                let st = cx.new(|cx| {
                    GraphEditor::new(
                        op.instance().required_spacing_graph().clone(),
                        REQUIRED_SPACING_GRAPH_NODES.clone(),
                        cx,
                    )
                });
                self.editor_state = Some(EditorState::Main(st));
            }
        });
    }

    fn on_select_main_graph(&mut self, _: &ClickEvent, _: &mut Window, cx: &mut Context<Self>) {
        cx.update_global::<CurrentBrushPresetOperator, _>(|op, cx| {
            let Some(op) = op.as_ref() else {
                return;
            };

            if let Some(Selected::Brush(brush)) = &mut self.selected {
                brush.viewing_graph = BrushPresetGraph::Main;
                let st = cx.new(|cx| {
                    GraphEditor::new(
                        op.instance().main_graph().clone(),
                        MAIN_GRAPH_NODES.clone(),
                        cx,
                    )
                });
                self.editor_state = Some(EditorState::Main(st));
            }
        });
    }

    fn on_select_stroke_pp_graph(
        &mut self,
        index: usize,
        _: &ClickEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        cx.update_global::<CurrentBrushPresetOperator, _>(|op, cx| {
            let Some(op) = op.as_ref() else {
                return;
            };
            let Some(graph) = op.instance().stroke_postprocess_graph(index) else {
                return;
            };
            if let Some(Selected::Brush(brush)) = &mut self.selected {
                brush.viewing_graph = BrushPresetGraph::StrokePostprocess { index };
                let st = cx.new(|cx| {
                    GraphEditor::new(graph.clone(), STROKE_POSTPROCESS_GRAPH_NODES.clone(), cx)
                });
                self.editor_state = Some(EditorState::Postprocess(st));
            }
        });
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
                Selected::Brush(_) => {
                    cx.update_global::<CurrentBrushPresetOperator, _>(|op, cx| {
                        let Some(op) = op.as_mut() else {
                            return div().into_any_element();
                        };

                        let instance = op.instance();
                        let brush_extra = self.render_brush_extra_panes(instance, window, cx);
                        h_flex()
                            .gap_2()
                            .size_full()
                            .child(common_editor)
                            .child(brush_extra)
                            .into_any_element()
                    })
                }
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

pub enum EditorState {
    Main(Entity<GraphEditor<BrushGraphData>>),
    Postprocess(Entity<GraphEditor<BrushGraphPostprocessData>>),
}
