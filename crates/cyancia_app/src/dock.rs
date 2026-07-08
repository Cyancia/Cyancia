use cyancia_canvas::{
    CanvasAppExt, CanvasId,
    event::CurrentCanvasChanged,
    tools::PanTool,
    widget::{canvas::CanvasWidget, layer_stack::LayerStackWidget},
};
use cyancia_tools::{ToolFunction, ToolProxies, ToolProxyId};
use gpui::{
    App, AppContext, BorrowAppContext, Context, Entity, EventEmitter, FocusHandle, Focusable,
    IntoElement, Render, SharedString, Window, div,
};
use gpui_component::dock::{Panel, PanelEvent};

macro_rules! test_dummy_dock {
    ($name:ident) => {
        pub struct $name {
            focus_handle: FocusHandle,
        }

        impl $name {
            pub fn new(cx: &mut Context<Self>) -> Self {
                Self {
                    focus_handle: cx.focus_handle(),
                }
            }
        }

        impl Panel for $name {
            fn panel_name(&self) -> &'static str {
                stringify!($name)
            }
        }

        impl EventEmitter<PanelEvent> for $name {}

        impl Focusable for $name {
            fn focus_handle(&self, _: &App) -> FocusHandle {
                self.focus_handle.clone()
            }
        }

        impl Render for $name {
            fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
                format!("Hello {}", stringify!($name))
            }
        }
    };
}

test_dummy_dock!(LayersDock);
test_dummy_dock!(FiltersDock);

pub struct CanvasDock {
    canvas: CanvasId,
    focus_handle: FocusHandle,
    canvas_state: Entity<CanvasWidget>,
}

impl CanvasDock {
    pub fn new(
        canvas_id: CanvasId,
        tool_proxy_id: ToolProxyId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        cx.update_global::<ToolProxies, _>(|tool_proxies, cx| {
            tool_proxies
                .get_mut(&tool_proxy_id)
                .switch_tool(PanTool::id(), cx);
        });

        let canvas_state =
            cx.new(|cx| CanvasWidget::new(canvas_id, tool_proxy_id, window, cx, true).unwrap());

        canvas_state.update(cx, |widget, cx| {
            widget.recomposite(cx);
        });

        Self {
            canvas: canvas_id,
            focus_handle: cx.focus_handle(),
            canvas_state,
        }
    }
}

impl EventEmitter<PanelEvent> for CanvasDock {}

impl Focusable for CanvasDock {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Panel for CanvasDock {
    fn panel_name(&self) -> &'static str {
        construct_canvas_dock_id(self.canvas).leak()
    }

    fn tab_name(&self, _: &App) -> Option<SharedString> {
        // TODO Record the filename of an image and display it
        Some(format!("Canvas {}", self.canvas).into())
    }
}

pub fn construct_canvas_dock_id(canvas: CanvasId) -> String {
    format!("canvas_dock_{}", canvas)
}

impl Render for CanvasDock {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        self.canvas_state.clone()
    }
}

pub struct CurrentCanvasLayersDock {
    widget: Option<Entity<LayerStackWidget>>,
    focus_handle: FocusHandle,
}

impl CurrentCanvasLayersDock {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus_handle = cx.focus_handle();
        cx.subscribe_in(
            &cx.global_canvas_events_entity(),
            window,
            |dock, _, _: &CurrentCanvasChanged, window, cx| {
                if let Some(canvas) = cx.current_canvas().and_then(|e| e.upgrade()) {
                    dock.widget = Some(cx.new(|cx| LayerStackWidget::new(canvas, window, cx)));
                }
            },
        )
        .detach();

        Self {
            widget: None,
            focus_handle,
        }
    }
}

impl EventEmitter<PanelEvent> for CurrentCanvasLayersDock {}

impl Focusable for CurrentCanvasLayersDock {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Panel for CurrentCanvasLayersDock {
    fn panel_name(&self) -> &'static str {
        "current_canvas_layers"
    }

    fn tab_name(&self, _: &App) -> Option<SharedString> {
        Some("Current Canvas Layers".into())
    }
}

impl Render for CurrentCanvasLayersDock {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        self.widget
            .as_ref()
            .map(|w| w.clone().into_any_element())
            .unwrap_or_else(|| div().into_any_element())
    }
}

pub struct ToolOptionsDock {
    focus_handle: FocusHandle,
}

impl ToolOptionsDock {
    pub fn new(_: &mut Window, cx: &mut Context<Self>) -> Self {
        Self {
            focus_handle: cx.focus_handle(),
        }
    }
}

impl EventEmitter<PanelEvent> for ToolOptionsDock {}

impl Focusable for ToolOptionsDock {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Panel for ToolOptionsDock {
    fn panel_name(&self) -> &'static str {
        "tool_options"
    }

    fn title(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        "Tool Options"
    }
}

impl Render for ToolOptionsDock {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(canvas) = cx.read_current_canvas() else {
            return div().into_any_element();
        };

        let tool_proxy_id = canvas.tool_proxy_id();
        cx.update_global::<ToolProxies, _>(|tool_proxies, cx| {
            let tool_proxy = tool_proxies.get_mut(&tool_proxy_id);
            tool_proxy
                .tool_option_widget(window, cx)
                .unwrap_or_else(|| div().into_any_element())
        })
    }
}
