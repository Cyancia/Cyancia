use std::borrow::BorrowMut;

use bevy_math::IRect;
use cyancia_image::{
    composite::{BlendFunctionId, BlendFunctionRegistry},
    layer::LayerId,
    tile::GpuTileStorage,
};
use cyancia_undo::UndoStacks;
use glam::IVec2;
use gpui::{
    App, AppContext, BorrowAppContext, Bounds, Context, Entity, InteractiveElement, IntoElement,
    ParentElement, Pixels, Point, Render, SharedString, StatefulInteractiveElement, Styled,
    Subscription, WeakEntity, Window, div, prelude::FluentBuilder, px,
};
use gpui_component::{
    ActiveTheme, h_flex,
    input::{Input, InputState},
    scroll::ScrollableElement,
    select::{SearchableVec, Select, SelectEvent, SelectState},
    v_flex,
};

use crate::{
    CCanvas,
    command::MoveLayerCommand,
    event::{CanvasActiveLayerChanged, CanvasLayerStackUpdated, CanvasUpdated},
};

pub struct LayerStackWidget {
    canvas: WeakEntity<CCanvas>,

    rename_input_state: Entity<InputState>,
    renaming_layer: Option<LayerId>,
    blend_mode_select_state: Entity<SelectState<SearchableVec<BlendFunctionId>>>,
    layer_widget_bounds: Vec<Bounds<Pixels>>,
    /// (layer_id, depth) in display order
    display_order: Vec<(LayerId, u32)>,

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
            layer_widget_bounds: Vec::new(),
            display_order: Vec::new(),
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

    fn on_layer_drop(&mut self, info: &LayerDragInfo, window: &mut Window, cx: &mut Context<Self>) {
        let dragged_id = info.id;
        let mouse_pos = window.mouse_position();

        let Some(target_index) = self.layer_widget_bounds.iter().position(|bounds| {
            mouse_pos.y >= bounds.origin.y && mouse_pos.y < bounds.origin.y + bounds.size.height
        }) else {
            return;
        };

        let Some(&(ref reference_id, depth)) = self.display_order.get(target_index) else {
            return;
        };

        if *reference_id == dragged_id {
            return;
        }

        let Some(canvas_entity) = self.canvas.upgrade() else {
            return;
        };
        let canvas = canvas_entity.read(cx);
        let layer_stack = canvas.image.layer_stack();

        let bounds = &self.layer_widget_bounds[target_index];
        let center_y = bounds.origin.y + bounds.size.height / 2.0;
        let position = if mouse_pos.y < center_y {
            DropPosition::Above
        } else {
            self.resolve_below_or_nest(
                mouse_pos.x,
                dragged_id,
                *reference_id,
                depth,
                target_index,
                bounds,
                layer_stack,
            )
        };

        let (new_parent, new_index) = match position {
            DropPosition::Above => {
                let node = layer_stack.find_node(*reference_id);
                let Some(node) = node else { return };
                let Some(parent) = node.parent() else { return };
                let idx_in_parent = layer_stack
                    .find_node(parent)
                    .and_then(|p| p.child_index(*reference_id))
                    .unwrap_or(0);
                (parent, idx_in_parent + 1)
            }
            DropPosition::Below => {
                let node = layer_stack.find_node(*reference_id);
                let Some(node) = node else { return };
                let Some(parent) = node.parent() else { return };
                let idx_in_parent = layer_stack
                    .find_node(parent)
                    .and_then(|p| p.child_index(*reference_id))
                    .unwrap_or(0);
                (parent, idx_in_parent)
            }
            DropPosition::AsChild => {
                if layer_stack.is_ancestor(dragged_id, *reference_id) {
                    return;
                }
                if !layer_stack
                    .can_have_children_of(*reference_id, dragged_id)
                    .unwrap_or(false)
                {
                    return;
                }
                let n_children = layer_stack
                    .find_node(*reference_id)
                    .map(|n| n.n_children())
                    .unwrap_or(0);
                (*reference_id, n_children)
            }
        };

        let Some(dragged_node) = layer_stack.find_node(dragged_id) else {
            return;
        };
        let Some(original_parent) = dragged_node.parent() else {
            return;
        };
        let original_index = layer_stack
            .find_node(original_parent)
            .and_then(|p| p.child_index(dragged_id))
            .unwrap_or(0);

        if original_parent == new_parent && original_index == new_index {
            return;
        }

        let canvas_id = canvas.id();

        let command = MoveLayerCommand {
            canvas: canvas_id,
            layer: dragged_id,
            original_parent,
            original_index,
            new_parent,
            new_index,
        };

        let result = cx.update_global::<UndoStacks, _>(|stacks, cx| {
            let app: &mut App = cx.borrow_mut();
            let Some(stack) = stacks.get_mut(&canvas_id) else {
                return Err(anyhow::anyhow!(
                    "Undo stack not found for canvas {}",
                    canvas_id
                ));
            };
            stack.push_boxed(Box::new(command), app)
        });

        if result.is_ok() {
            canvas_entity.update(cx, |_, cx| {
                cx.emit(CanvasLayerStackUpdated {});
            });
        }
    }

    fn resolve_below_or_nest(
        &self,
        mouse_x: Pixels,
        dragged_id: LayerId,
        reference_id: LayerId,
        depth: u32,
        target_index: usize,
        bounds: &Bounds<Pixels>,
        layer_stack: &cyancia_image::layer::LayerStack,
    ) -> DropPosition {
        let is_last_child = self
            .display_order
            .get(target_index + 1)
            .map_or(true, |(_, next_depth)| *next_depth < depth);

        if !is_last_child {
            return DropPosition::Below;
        }

        let Some(group_id) = layer_stack.find_node(reference_id).and_then(|n| n.parent()) else {
            return DropPosition::Below;
        };

        if !layer_stack
            .can_have_children_of(group_id, dragged_id)
            .unwrap_or(false)
        {
            return DropPosition::Below;
        }

        let child_indent_x = bounds.origin.x;
        let group_indent_x = child_indent_x - px(20.0);

        let dist_to_group = (mouse_x.as_f32() - group_indent_x.as_f32()).abs();
        let dist_to_child = (mouse_x.as_f32() - child_indent_x.as_f32()).abs();

        if dist_to_child < dist_to_group {
            DropPosition::AsChild
        } else {
            DropPosition::Below
        }
    }
}

impl Render for LayerStackWidget {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(canvas_entity) = self.canvas.upgrade() else {
            return div().into_any_element();
        };

        let canvas = canvas_entity.read(cx);

        self.display_order = canvas
            .image
            .layer_stack()
            .iter_layers_dfs_display_order_without_root()
            .map(|(layer, depth)| (layer.id(), depth))
            .collect();

        let layers = canvas
            .image
            .layer_stack()
            .iter_layers_dfs_display_order_without_root()
            .map(|(layer, depth)| {
                let drag_info = LayerDragInfo::new(layer.id(), layer.name.clone().into());
                h_flex()
                    .pl(px(20.0 * depth as f32))
                    .h(px(40.0))
                    .when(canvas.active_layer_id() == layer.id(), |d| {
                        d.bg(cx.theme().accent)
                    })
                    .id(format!("layer-{}", layer.id()))
                    .when_else(
                        Some(layer.id()) == self.renaming_layer,
                        |d| d.child(Input::new(&self.rename_input_state).w_full()),
                        |d| d.child(layer.name.clone()),
                    )
                    .on_drag(drag_info, |info, position, _, cx| {
                        cx.new(|_| info.clone().with_position(position))
                    })
                    .on_click({
                        let canvas_entity = canvas_entity.downgrade();
                        let layer_id = layer.id();
                        move |_, _, cx| {
                            canvas_entity
                                .update(cx, |canvas, cx| {
                                    canvas.set_active_layer(layer_id, cx);
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
                    .on_children_prepainted({
                        let widget = cx.entity().downgrade();
                        move |bounds, window, cx| {
                            widget
                                .update(cx, |widget, cx| {
                                    widget.layer_widget_bounds = bounds;
                                })
                                .ok();
                        }
                    })
                    .on_drop(cx.listener(Self::on_layer_drop))
                    .overflow_scrollbar(),
            )
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
