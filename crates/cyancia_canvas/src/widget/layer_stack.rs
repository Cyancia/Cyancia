use bevy_math::IRect;
use cyancia_image::{
    composite::{BlendFunctionId, BlendFunctionRegistry},
    layer::LayerId,
    tile::GpuTileStorage,
};
use cyancia_utils::log_err::LogErr;
use cyancia_widgets::spin_slider::{SpinSlider, SpinSliderEvent, SpinSliderState};
use glam::IVec2;
use gpui::{
    Action, AppContext, Bounds, Context, DragMoveEvent, Entity, Focusable, InteractiveElement,
    IntoElement, Modifiers, ParentElement, Pixels, Point, Render, SharedString,
    StatefulInteractiveElement, Styled, Subscription, WeakEntity, Window, actions, div,
    prelude::FluentBuilder, px,
};
use gpui_component::{
    ActiveTheme, ElementExt, Sizable, StyledExt, h_flex,
    input::{Input, InputEvent, InputState},
    menu::{ContextMenuExt, PopupMenuItem},
    scroll::ScrollableElement,
    select::{SearchableVec, Select, SelectEvent, SelectState},
    v_flex,
};
use indexmap::IndexMap;
use schemars::JsonSchema;
use serde::Deserialize;

use crate::{
    CCanvas, CanvasUndoStackAppExt,
    command::{MoveLayerCommand, RenameLayerCommand},
    event::{CanvasActiveLayerChanged, CanvasLayerStackUpdated, CanvasUpdated},
};

pub const LAYER_STACK_CONTEXT: &'static str = "layer_stack";

#[derive(Action, Clone, PartialEq, JsonSchema, Deserialize)]
pub struct RenameLayer {
    pub layer_id: LayerId,
}

#[derive(Debug, Clone)]
struct LayerWidgetInfo {
    layer_id: LayerId,
    bounds: Bounds<Pixels>,
}

struct DropInfo {
    parent: LayerId,
    index: usize,
    position: Point<Pixels>,
    length: Pixels,
}

pub struct LayerStackWidget {
    canvas: WeakEntity<CCanvas>,

    rename_input_state: Entity<InputState>,
    renaming_layer: Option<LayerId>,
    blend_mode_select_state: Entity<SelectState<SearchableVec<BlendFunctionId>>>,
    layer_widget_info: IndexMap<LayerId, LayerWidgetInfo>,
    layer_drop_info: Option<DropInfo>,
    layer_drop_indicator_offset: Point<Pixels>,
    opacity_state: Entity<SpinSliderState>,

    _subscriptions: Vec<Subscription>,
}

impl LayerStackWidget {
    pub fn new(canvas: Entity<CCanvas>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let blend_mode_select_state = cx.new(|cx| {
            let funcs = BlendFunctionRegistry::global(cx)
                .all_ids()
                .cloned()
                .collect::<Vec<_>>();
            SelectState::new(funcs.into(), None, window, cx)
        });

        let rename_input_state = cx.new(|cx| InputState::new(window, cx));
        let opacity_state = cx.new(|cx| SpinSliderState::new_percent(window, cx));

        let subscriptions = vec![
            cx.subscribe_in(
                &canvas,
                window,
                |_, _, _: &CanvasLayerStackUpdated, _, cx| {
                    cx.notify();
                },
            ),
            cx.subscribe_in(&canvas, window, Self::on_active_layer_changed),
            cx.observe_global::<BlendFunctionRegistry>(Self::on_blend_function_registry_changed),
            cx.subscribe_in(
                &blend_mode_select_state,
                window,
                Self::on_blend_function_changed,
            ),
            cx.subscribe_in(&rename_input_state, window, Self::on_rename_input_event),
            cx.subscribe_in(&opacity_state, window, Self::on_opacity_changed),
        ];

        Self {
            rename_input_state,
            renaming_layer: None,
            canvas: canvas.downgrade(),
            blend_mode_select_state,
            layer_widget_info: IndexMap::new(),
            layer_drop_info: None,
            layer_drop_indicator_offset: Point::default(),
            opacity_state,
            _subscriptions: subscriptions,
        }
    }

    fn on_blend_function_changed(
        &mut self,
        select_state: &Entity<SelectState<SearchableVec<BlendFunctionId>>>,
        event: &SelectEvent<SearchableVec<BlendFunctionId>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            SelectEvent::Confirm(Some(value)) => {
                self.canvas
                    .update(cx, |canvas, cx| {
                        canvas.active_layer_node_mut().data_mut().blend_func = value.clone();
                        // TODO use layer bound
                        let dirty_tiles = GpuTileStorage::pixel_rect_to_tile(IRect {
                            min: IVec2::ZERO,
                            max: canvas.image.size().as_ivec2(),
                        });
                        cx.emit(CanvasUpdated { dirty_tiles });
                    })
                    .ok();
            }
            _ => {}
        }
    }

    fn on_active_layer_changed(
        &mut self,
        canvas: &Entity<CCanvas>,
        event: &CanvasActiveLayerChanged,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let active_layer = canvas.read(cx).active_layer_node();

        let blend_func = active_layer.data().blend_func.clone();
        self.blend_mode_select_state.update(cx, |state, cx| {
            state.set_selected_value(&blend_func, window, cx);
        });
    }

    fn on_blend_function_registry_changed(&mut self, cx: &mut Context<Self>) {
        self.blend_mode_select_state.update(cx, |state, cx| {
            let funcs = BlendFunctionRegistry::global(cx)
                .all_ids()
                .cloned()
                .collect::<Vec<_>>();

            // TODO This is hacky. gpui-component is not using the window passed in.
            #[allow(invalid_value)]
            let dummy_window = unsafe { std::mem::zeroed() };
            state.set_items(funcs.into(), dummy_window, cx);
        });
    }

    fn on_layer_drag_move(
        &mut self,
        event: &DragMoveEvent<LayerDragInfo>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.layer_drop_info =
            self.resolve_drop_target(event.dragged_item().downcast_ref().unwrap(), window, cx);
    }

    fn on_layer_drop(&mut self, info: &LayerDragInfo, window: &mut Window, cx: &mut Context<Self>) {
        let Some(drop_info) = self.layer_drop_info.take() else {
            return;
        };

        let canvas_entity = self.canvas.upgrade().unwrap();
        let canvas = canvas_entity.read(cx);
        let layer_stack = canvas.image.layer_stack();

        let Some(dragged_node) = layer_stack.get_layer(&info.id) else {
            return;
        };
        let Some(original_parent) = dragged_node.parent().copied() else {
            return;
        };
        let original_index = layer_stack
            .get_layer(&original_parent)
            .and_then(|p| p.child_index(&info.id))
            .unwrap_or(0);

        if original_parent == drop_info.parent && original_index == drop_info.index {
            return;
        }

        let canvas_id = canvas.id();

        let command = MoveLayerCommand {
            canvas: canvas_id,
            layer: info.id,
            original_parent,
            original_index,
            new_parent: drop_info.parent,
            new_index: drop_info.index,
        };

        cx.push_undo_command(&canvas_id, command).ok();
        canvas_entity.update(cx, |_, cx| {
            cx.emit(CanvasLayerStackUpdated {});
        });
    }

    fn on_opacity_changed(
        &mut self,
        state: &Entity<SpinSliderState>,
        event: &SpinSliderEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            SpinSliderEvent::Release(val) => {
                dbg!(val);
            }
            _ => {}
        }
    }

    fn resolve_drop_target(
        &self,
        info: &LayerDragInfo,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<DropInfo> {
        let mouse_pos = window.mouse_position();

        let LayerWidgetInfo {
            layer_id: target_id,
            bounds: target_bounds,
        } = self
            .layer_widget_info
            .values()
            .find(|l| mouse_pos.y < l.bounds.bottom())?
            .clone();

        if target_id == info.id {
            return None;
        }

        let canvas_entity = self.canvas.upgrade()?;
        let canvas = canvas_entity.read(cx);
        let layer_stack = canvas.image.layer_stack();

        if layer_stack.is_ancestor(&info.id, &target_id) {
            return None;
        }

        let target_node = layer_stack.get_layer(&target_id)?;
        let center_y = target_bounds.center().y;
        let old_index = layer_stack
            .get_layer(&info.id)
            .and_then(|n| n.parent())
            .and_then(|p| layer_stack.get_layer(p))
            .and_then(|p| p.child_index(&info.id))?;

        if mouse_pos.y < center_y {
            // Above the target's row: drop as a sibling, just above the target.
            let parent = target_node.parent().copied()?;
            let target_index = layer_stack.get_layer(&parent)?.child_index(&target_id)?;
            let dragged_parent = layer_stack.get_layer(&info.id)?.parent().copied()?;
            let index = if dragged_parent == parent && old_index < target_index {
                target_index
            } else {
                target_index + 1
            };
            return Some(DropInfo {
                parent,
                index,
                position: target_bounds.origin,
                length: target_bounds.size.width,
            });
        }

        // Below the target's row.
        let target_parent = target_node.parent().copied()?;
        let target_parent_node = layer_stack.get_layer(&target_parent)?;
        let target_index = target_parent_node.child_index(&target_id)?;
        let dragged_parent = layer_stack.get_layer(&info.id)?.parent().copied()?;

        // If the target can hold children and the cursor is at the target's own
        // indent (inside the target's row, not in a shallower ancestor's gutter),
        // dropping on its lower half nests the dragged layer as the target's new
        // top child (preview at the target's bottom edge). A shallower cursor
        // falls through to the x-driven ancestor matching below.
        if mouse_pos.x >= target_bounds.left()
            && layer_stack.can_have_children_of(&target_id, &info.id)?
        {
            let n_children = layer_stack.get_layer(&target_id)?.n_children();
            return Some(DropInfo {
                parent: target_id,
                index: n_children,
                position: target_bounds.bottom_left(),
                length: target_bounds.size.width,
            });
        }

        // The target can't hold children (or the cursor is at a shallower indent),
        // so the dragged layer lands below it as a sibling. When the target is the
        // bottom child of its parent, "below it" is ambiguous: it could mean a new
        // bottom of the target's parent, or the bottom of an ancestor further up.
        // The cursor's horizontal indent picks which ancestor's bottom to append to.
        if target_index != 0 {
            // Target has a lower sibling, so "below the target" stays inside the
            // target's parent, just under the target.
            return Some(DropInfo {
                parent: target_parent,
                index: if dragged_parent == target_parent && old_index < target_index {
                    target_index - 1
                } else {
                    target_index
                },
                position: target_bounds.bottom_left(),
                length: target_bounds.size.width,
            });
        }

        // Target is the bottom child. Only ancestors for which the target's
        // branch is also the bottom child are valid "append to bottom" targets.
        let ancestors = layer_stack.ancestors(&target_id); // target -> root, target excluded
        let mut ambiguous_count = 0;
        {
            let mut previous_child = target_id;
            for ancestor in &ancestors {
                let ancestor_node = layer_stack.get_layer(ancestor)?;
                if ancestor_node.child_index(&previous_child) == Some(0) {
                    ambiguous_count += 1;
                } else {
                    break;
                }
                previous_child = *ancestor;
            }
        }

        // The deepest candidate ancestor whose row the cursor is still inside is
        // the new parent; the dragged layer becomes its new bottom child. If the
        // cursor is left of every candidate, append to the shallowest one.
        let mut resolved_parent_index = ambiguous_count - 1;
        for (index, ancestor) in ancestors.iter().take(ambiguous_count).enumerate() {
            let bounds = self.layer_widget_info.get(ancestor)?.bounds;
            if mouse_pos.x >= bounds.left() {
                resolved_parent_index = index;
                break;
            }
        }
        let resolved_parent = ancestors[resolved_parent_index];
        if layer_stack.is_ancestor(&info.id, &resolved_parent) {
            return None;
        }

        let resolved_preview_bounds = if resolved_parent_index == 0 {
            target_bounds
        } else {
            self.layer_widget_info
                .get(&ancestors[resolved_parent_index - 1])
                .unwrap()
                .bounds
        };

        Some(DropInfo {
            parent: resolved_parent,
            index: 0,
            position: Point::new(resolved_preview_bounds.origin.x, target_bounds.bottom()),
            length: resolved_preview_bounds.size.width,
        })
    }

    fn on_rename_layer(
        &mut self,
        action: &RenameLayer,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let layer_name = self
            .canvas
            .read_with(cx, |canvas, cx| {
                let layer = canvas.image.layer_stack().get_layer(&action.layer_id)?;
                Some(layer.data().name.clone())
            })
            .ok()
            .flatten();

        let Some(layer_name) = layer_name else {
            return;
        };

        self.rename_input_state.update(cx, |state, cx| {
            state.set_value(layer_name, window, cx);
            cx.focus_self(window);
        });
        self.renaming_layer = Some(action.layer_id);
    }

    fn on_rename_input_event(
        &mut self,
        state: &Entity<InputState>,
        event: &InputEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            InputEvent::PressEnter { .. } | InputEvent::Blur => {
                let Some(layer_id) = self.renaming_layer.take() else {
                    return;
                };
                let value = state.read(cx).value();
                let Some(canvas) = self.canvas.upgrade() else {
                    return;
                };
                let canvas = canvas.read(cx);
                let Some(layer) = canvas.image.layer_stack().get_layer(&layer_id) else {
                    return;
                };

                let cmd = RenameLayerCommand {
                    canvas: canvas.id(),
                    layer_id,
                    new_name: value.into(),
                    old_name: layer.data().name.clone(),
                };
                cx.push_undo_command_to_current(cmd).log_err();
            }
            InputEvent::Change | InputEvent::Focus => {}
        }
    }
}

impl Render for LayerStackWidget {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(canvas_entity) = self.canvas.upgrade() else {
            return div().into_any_element();
        };

        self.layer_widget_info.clear();

        let canvas = canvas_entity.read(cx);

        let widget = cx.entity().downgrade();
        let layers = canvas
            .image
            .layer_stack()
            .iter_layers_dfs_display_order_without_root()
            .map(|(node, depth)| {
                let layer_id = *node.id();
                let drag_info = LayerDragInfo::new(layer_id, node.data().name.clone().into());

                let inner = h_flex()
                    .size_full()
                    .ml(px(20.0 * depth as f32))
                    .when_else(
                        Some(layer_id) == self.renaming_layer,
                        |d| d.child(Input::new(&self.rename_input_state).w_full()),
                        |d| d.child(node.data().name.clone()),
                    )
                    .on_prepaint({
                        let widget = widget.clone();
                        move |bounds, window, cx| {
                            widget
                                .update(cx, |widget, cx| {
                                    widget.layer_widget_info.insert(
                                        layer_id,
                                        LayerWidgetInfo {
                                            layer_id: layer_id,
                                            bounds,
                                        },
                                    );
                                })
                                .ok();
                        }
                    });

                h_flex()
                    .h(px(40.0))
                    .when(canvas.selected_layer_ids().contains(&layer_id), |d| {
                        d.bg(cx.theme().accent)
                    })
                    .when(canvas.active_layer == layer_id, |d| d.font_bold())
                    .id(format!("layer-{}", layer_id))
                    .on_drag(drag_info, |info, position, _, cx| {
                        cx.new(|_| info.clone().with_position(position))
                    })
                    .on_drag_move(cx.listener(Self::on_layer_drag_move))
                    .on_click({
                        let canvas_entity = canvas_entity.downgrade();
                        move |event, _, cx| {
                            canvas_entity
                                .update(cx, |canvas, cx| {
                                    if event.modifiers() == Modifiers::control() {
                                        canvas.toggle_layer_selection_and_active(layer_id, cx);
                                    } else if event.modifiers() == Modifiers::shift() {
                                        let active_layer = canvas.active_layer_id();
                                        if layer_id == active_layer {
                                            return;
                                        }
                                        let tree = canvas
                                            .image
                                            .layer_stack()
                                            .iter_layers_dfs_display_order_without_root()
                                            .map(|(n, _)| *n.id())
                                            .collect::<Vec<_>>();
                                        let mut on_select = false;
                                        for layer in tree {
                                            if on_select {
                                                canvas.select_layer(layer);
                                            }
                                            if layer == layer_id || layer == active_layer {
                                                on_select = !on_select;
                                            }
                                        }
                                        canvas.set_active_layer(layer_id, cx);
                                    } else {
                                        canvas.set_active_layer_and_clear_select(layer_id, cx);
                                    }
                                })
                                .ok();
                        }
                    })
                    .on_action(cx.listener(Self::on_rename_layer))
                    .context_menu(move |menu, window, cx| {
                        menu.item(
                            PopupMenuItem::new("Rename").action(Box::new(RenameLayer { layer_id })),
                        )
                    })
                    .child(inner)
            });

        let layer_params = v_flex()
            .p_2()
            .child(Select::new(&self.blend_mode_select_state))
            .child(
                SpinSlider::new(&self.opacity_state)
                    .small()
                    .prefix("Opacity: ")
                    .suffix("%"),
            );

        v_flex()
            .key_context(LAYER_STACK_CONTEXT)
            .w_full()
            .h_full()
            .gap_2()
            .child(layer_params)
            .child(
                div()
                    .size_full()
                    .children(layers)
                    .on_drop(cx.listener(Self::on_layer_drop))
                    .overflow_scrollbar(),
            )
            .when_some(self.layer_drop_info.as_ref(), |d, info| {
                d.on_prepaint({
                    let widget = widget.clone();
                    move |bounds, window, cx| {
                        widget
                            .update(cx, |widget, cx| {
                                widget.layer_drop_indicator_offset = bounds.origin;
                            })
                            .ok();
                    }
                })
                .child(
                    div()
                        .w(info.length)
                        .absolute()
                        .left(info.position.x - self.layer_drop_indicator_offset.x)
                        .top(info.position.y - self.layer_drop_indicator_offset.y)
                        .border_1()
                        .border_color(cx.theme().accent_foreground),
                )
            })
            .into_any_element()
    }
}

#[derive(Debug, Clone)]
pub struct LayerDragInfo {
    id: LayerId,
    position: Point<Pixels>,
    layer_name: SharedString,
}

impl LayerDragInfo {
    pub fn new(id: LayerId, name: SharedString) -> Self {
        Self {
            id,
            position: Default::default(),
            layer_name: name,
        }
    }

    pub fn with_position(mut self, position: Point<Pixels>) -> Self {
        self.position = position;
        self
    }
}

impl Render for LayerDragInfo {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .bg(cx.theme().background)
            .p_2()
            .child(self.layer_name.clone())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExpandState {
    Expanded,
    Collapsed,
    Unexpandable,
}
