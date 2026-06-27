use bevy_math::IRect;
use cyancia_image::{
    blend_modes::BlendMode,
    composite::{BlendFunctionId, BlendFunctionRegistry},
    layer::LayerId,
    tile::GpuTileStorage,
};
use glam::IVec2;
use gpui::{
    AppContext, Context, Entity, InteractiveElement, IntoElement, ParentElement, Pixels, Point,
    Render, SharedString, StatefulInteractiveElement, Styled, Subscription, WeakEntity, Window,
    div, prelude::FluentBuilder, px,
};
use gpui_component::{
    ActiveTheme, h_flex,
    input::{Input, InputState},
    scroll::ScrollableElement,
    select::{SearchableVec, Select, SelectEvent, SelectState},
    v_flex,
};
use tracing::error;

use crate::{
    CCanvas,
    event::{CanvasActiveLayerChanged, CanvasLayerStackUpdated, CanvasUpdated},
};

pub struct LayerStackWidget {
    canvas: WeakEntity<CCanvas>,
    rename_input_state: Entity<InputState>,
    renaming_layer: Option<LayerId>,
    blend_mode_select_state: Entity<SelectState<SearchableVec<BlendFunctionId>>>,
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
}

impl Render for LayerStackWidget {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(canvas_entity) = self.canvas.upgrade() else {
            return div().into_any_element();
        };

        let canvas = canvas_entity.read(cx);

        let layers = canvas
            .image
            .layer_stack()
            .iter_layers_dfs_display_order_without_root()
            .map(|(layer, depth)| {
                let drag_info = LayerDragInfo::new(layer.name.clone().into());
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
            .overflow_scrollbar()
            .gap_2()
            .child(layer_params)
            .children(layers)
            .into_any_element()
    }
}

#[derive(Clone)]
pub struct LayerDragInfo {
    position: Point<Pixels>,
    layer_name: SharedString,
}

impl LayerDragInfo {
    pub fn new(name: SharedString) -> Self {
        Self {
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
