use cyancia_image::layer::LayerId;
use gpui::{
    AppContext, Context, Entity, InteractiveElement, IntoElement, ParentElement, Pixels, Point,
    Render, SharedString, StatefulInteractiveElement, Styled, Subscription, WeakEntity, Window,
    div, prelude::FluentBuilder, px,
};
use gpui_component::{
    ActiveTheme, h_flex,
    input::{Input, InputState},
    scroll::ScrollableElement,
    v_flex,
};

use crate::{CCanvas, event::CanvasLayerStackUpdated};

pub struct LayerStackWidget {
    canvas: WeakEntity<CCanvas>,
    rename_input_state: Entity<InputState>,
    renaming_layer: Option<LayerId>,
    _subscriptions: Vec<Subscription>,
}

impl LayerStackWidget {
    pub fn new(canvas: Entity<CCanvas>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let subscriptions = vec![cx.subscribe_in(
            &canvas,
            window,
            |_, _, _: &CanvasLayerStackUpdated, _, cx| {
                cx.notify();
            },
        )];

        Self {
            rename_input_state: cx.new(|cx| InputState::new(window, cx)),
            renaming_layer: None,
            canvas: canvas.downgrade(),
            _subscriptions: subscriptions,
        }
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
                    .when(canvas.image.active_layer == layer.id(), |d| {
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
                                    canvas.image.active_layer = layer_id;
                                    cx.emit(CanvasLayerStackUpdated {});
                                })
                                .ok();
                        }
                    })
            });

        v_flex()
            .w_full()
            .h_full()
            .overflow_scrollbar()
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
