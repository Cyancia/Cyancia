use std::rc::Rc;

use gpui::{
    App, AppContext, Context, Entity, EventEmitter, InteractiveElement, IntoElement, ParentElement,
    Pixels, Point, Render, SharedString, StatefulInteractiveElement, Styled, Window, div,
    prelude::FluentBuilder, px,
};
use gpui_component::{
    ActiveTheme, h_flex,
    input::{Input, InputState},
    scroll::ScrollableElement,
    v_flex,
};

use crate::{
    CImage,
    layer::{LayerId, LayerStack},
};

pub enum LayerStackEvent {
    LayerSelected(LayerId),
}

pub struct LayerStackWidget {
    rename_input_state: Entity<InputState>,
    renaming_layer: Option<LayerId>,
    on_event: Rc<dyn Fn(&LayerStackEvent, &mut Window, &mut App)>,
}

impl LayerStackWidget {
    pub fn new(
        window: &mut Window,
        cx: &mut App,
        on_event: Rc<dyn Fn(&LayerStackEvent, &mut Window, &mut App)>,
    ) -> Self {
        Self {
            rename_input_state: cx.new(|cx| InputState::new(window, cx)),
            renaming_layer: None,
            on_event,
        }
    }

    pub fn render_layer_stack(&self, image: &CImage, cx: &mut App) -> impl IntoElement {
        let layers = image
            .layer_stack()
            .iter_layers_dfs_display_order_without_root()
            .map(|(layer, depth)| {
                let drag_info = LayerDragInfo::new(layer.name.clone().into());
                h_flex()
                    .pl(px(20.0 * depth as f32))
                    .h(px(40.0))
                    .when(image.active_layer == layer.id(), |d| {
                        d.bg(cx.theme().accent)
                    })
                    .id(format!("layer-{}", layer.id()))
                    .when_else(
                        Some(layer.id()) == self.renaming_layer,
                        |d| d.child(Input::new(&self.rename_input_state).w_full()),
                        |d| d.child(layer.name.clone()),
                    )
                    .on_drag(drag_info, |info, position, window, cx| {
                        cx.new(|cx| info.clone().with_position(position))
                    })
                    .on_click({
                        let on_event = self.on_event.clone();
                        let layer_id = layer.id();
                        move |_, window, cx| {
                            on_event(&LayerStackEvent::LayerSelected(layer_id), window, cx);
                        }
                    })
            });

        v_flex()
            .w_full()
            .h_full()
            .overflow_scrollbar()
            .children(layers)
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
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .bg(cx.theme().background)
            .p_2()
            .child(self.layer_name.clone())
    }
}

#[derive(Debug, Clone)]
pub struct LayerItem {
    id: LayerId,
    name: String,
    depth: u32,
    selected: bool,
    expanded: ExpandState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExpandState {
    Expanded,
    Collapsed,
    Unexpandable,
}
