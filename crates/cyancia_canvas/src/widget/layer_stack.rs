use std::borrow::BorrowMut;

use bevy_math::IRect;
use cyancia_image::{
    composite::{BlendFunctionId, BlendFunctionRegistry},
    layer::{LayerId, LayerStack},
    tile::GpuTileStorage,
};
use cyancia_undo::UndoStacks;
use glam::IVec2;
use gpui::{
    App, AppContext, BorrowAppContext, Bounds, Context, DragMoveEvent, Entity, InteractiveElement,
    IntoElement, ParentElement, Pixels, Point, Render, SharedString, StatefulInteractiveElement,
    Styled, Subscription, WeakEntity, Window, div, prelude::FluentBuilder, px,
};
use gpui_component::{
    ActiveTheme, ElementExt, h_flex,
    input::{Input, InputState},
    scroll::ScrollableElement,
    select::{SearchableVec, Select, SelectEvent, SelectState},
    v_flex,
};
use indexmap::IndexMap;

use crate::{
    CCanvas, CanvasUndoStackAppExt,
    command::MoveLayerCommand,
    event::{CanvasActiveLayerChanged, CanvasLayerStackUpdated, CanvasUpdated},
};

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
        ];

        Self {
            rename_input_state: cx.new(|cx| InputState::new(window, cx)),
            renaming_layer: None,
            canvas: canvas.downgrade(),
            blend_mode_select_state,
            layer_widget_info: IndexMap::new(),
            layer_drop_info: None,
            layer_drop_indicator_offset: Point::default(),
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
                        canvas.active_layer_data_mut().blend_func = value.clone();
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
        let active_layer = canvas.read(cx).active_layer_data();

        let blend_func = active_layer.blend_func.clone();
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

        let Some(dragged_node) = layer_stack.find_node(info.id) else {
            return;
        };
        let Some(original_parent) = dragged_node.parent() else {
            return;
        };
        let original_index = layer_stack
            .find_node(original_parent)
            .and_then(|p| p.child_index(info.id))
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

        let center_y = target_bounds.center().y;

        if mouse_pos.y < center_y {
            // Above target layer
            let target_node = layer_stack.find_node(target_id)?;
            let target_node_parent_node = layer_stack.find_node(target_node.parent()?)?;
            let target_node_index = target_node_parent_node.child_index(target_id)?;
            Some(DropInfo {
                parent: target_node_parent_node.id(),
                index: target_node_index + 1,
                position: target_bounds.origin,
                length: target_bounds.size.width,
            })
        } else {
            // Below target layer

            let target_node = layer_stack.find_node(target_id)?;
            let target_node_parent_node = layer_stack.find_node(target_node.parent()?)?;
            let target_node_index = target_node_parent_node.child_index(target_id)?;

            // Target is not the child at bottom
            if target_node_index != 0 {
                let target_node = target_node_parent_node
                    .children()
                    .get(target_node_index - 1)?;
                let target_bounds = self.layer_widget_info.get(&target_node.id())?.bounds;

                if layer_stack.can_have_children_of(target_node.id(), info.id)? {
                    return Some(DropInfo {
                        parent: target_node.id(),
                        index: target_node.n_children(),
                        position: target_bounds.origin,
                        length: target_bounds.size.width,
                    });
                } else {
                    return Some(DropInfo {
                        parent: target_node_parent_node.id(),
                        index: target_node_index - 1,
                        position: target_bounds.origin,
                        length: target_bounds.size.width,
                    });
                }
            }

            // The mouse is right to the target, so insert after it.
            if mouse_pos.x > target_bounds.left() {
                if layer_stack.can_have_children_of(target_id, info.id)? {
                    return Some(DropInfo {
                        parent: target_id,
                        index: target_node.n_children(),
                        position: target_bounds.bottom_left(),
                        length: target_bounds.size.width,
                    });
                } else {
                    return Some(DropInfo {
                        parent: target_node_parent_node.id(),
                        index: target_node_index,
                        position: target_bounds.bottom_left(),
                        length: target_bounds.size.width,
                    });
                }
            }

            // The mouse is left to the target, and this is the bottom layer of the parent.
            // If we are trying to insert at the bottom of the parent of target layer,
            // it can be ambiguous to figure out which ancestor is going to be the parent.
            let ancestors = layer_stack.ancestors(target_id);
            let mut ambiguous_count = 0;
            {
                let mut previous_child = &target_id;
                for ancestor in &ancestors {
                    let ancestor_node = layer_stack.find_node(*ancestor)?;
                    // Previous child at the bottom of its parent.
                    if ancestor_node.child_index(*previous_child) == Some(0) {
                        ambiguous_count += 1;
                    } else {
                        break;
                    }
                    previous_child = ancestor;
                }
            }

            let mut resolved_sibling_index = 0;
            // Find the first ancestor that the mouse is to the right of, it is going to be the sibling,
            // on top of the dragged layer.
            for (index, ancestor) in ancestors.iter().take(ambiguous_count).enumerate() {
                let bounds = self.layer_widget_info.get(ancestor)?.bounds;
                if mouse_pos.x > bounds.left() {
                    resolved_sibling_index = index;
                    break;
                }
            }
            let resolved_sibling = ancestors[resolved_sibling_index];
            let resolved_sibling_bounds = self.layer_widget_info.get(&resolved_sibling)?.bounds;
            let resolved_parent = ancestors[resolved_sibling_index + 1];

            Some(DropInfo {
                parent: resolved_parent,
                index: if resolved_sibling_index + 1 == ambiguous_count {
                    // If the sibling is the last one, insert after that.
                    let resolved_parent_node = layer_stack.find_node(resolved_parent)?;
                    resolved_parent_node.child_index(resolved_sibling)?
                } else {
                    // Otherwise, insert at the bottom directly.
                    0
                },
                position: Point::new(resolved_sibling_bounds.origin.x, target_bounds.bottom()),
                length: resolved_sibling_bounds.size.width,
            })
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
            .map(|(layer, node, depth)| {
                let drag_info = LayerDragInfo::new(layer.id(), layer.name.clone().into());
                let layer_id = layer.id();

                h_flex()
                    .ml(px(20.0 * depth as f32))
                    .h(px(40.0))
                    .when(canvas.active_layer_id() == layer_id, |d| {
                        d.bg(cx.theme().accent)
                    })
                    .id(format!("layer-{}", layer_id))
                    .when_else(
                        Some(layer_id) == self.renaming_layer,
                        |d| d.child(Input::new(&self.rename_input_state).w_full()),
                        |d| d.child(layer.name.clone()),
                    )
                    .on_drag(drag_info, |info, position, _, cx| {
                        cx.new(|_| info.clone().with_position(position))
                    })
                    .on_drag_move(cx.listener(Self::on_layer_drag_move))
                    .on_click({
                        let canvas_entity = canvas_entity.downgrade();
                        move |_, _, cx| {
                            canvas_entity
                                .update(cx, |canvas, cx| {
                                    canvas.set_active_layer(layer_id, cx);
                                })
                                .ok();
                        }
                    })
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
                    })
            });

        let layer_params = v_flex()
            .p_2()
            .child(Select::new(&self.blend_mode_select_state));

        v_flex()
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

enum DropPosition {
    Above,
    Below,
    AsChild,
}
