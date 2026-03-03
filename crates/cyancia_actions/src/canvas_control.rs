use std::{marker::PhantomData, sync::Arc, time::Instant};

use async_trait::async_trait;
use cyancia_canvas::CanvasManager;
use cyancia_input::action::{Action, ActionId};
use cyancia_runtime::Services;
use cyancia_tools::{CanvasTool, CanvasToolId, CanvasToolProxies};

use crate::{ActionFunction};

pub trait CanvasToolAction: Send + Sync + 'static {
    fn action() -> ActionId;
    fn tool() -> CanvasToolId;
}

macro_rules! canvas_tool_action {
    ($name:ident, $action:literal, $tool: literal) => {
        #[derive(Default)]
        pub struct $name;
        impl CanvasToolAction for $name {
            fn action() -> ActionId {
                ActionId::new($action.into())
            }
            fn tool() -> CanvasToolId {
                CanvasToolId::new($tool.into())
            }
        }
    };
}
canvas_tool_action!(PanToolAction, "pan_tool", "pan_tool");
canvas_tool_action!(RotateToolAction, "rotate_tool", "rotate_tool");
canvas_tool_action!(ZoomToolAction, "zoom_tool", "zoom_tool");
canvas_tool_action!(BrushToolAction, "brush_tool", "brush_tool");

pub struct CanvasToolSwitch<T: CanvasToolAction> {
    activated: Instant,
    _marker: PhantomData<T>,
}

impl<T: CanvasToolAction> Default for CanvasToolSwitch<T> {
    fn default() -> Self {
        Self {
            activated: Instant::now(),
            _marker: PhantomData,
        }
    }
}

#[async_trait]
impl<T: CanvasToolAction> ActionFunction for CanvasToolSwitch<T> {
    fn id(&self) -> ActionId {
        T::action()
    }

    async fn trigger(&self, services: Arc<Services>) {
        let canvases = services.service::<CanvasManager>();
        let (Some(canvas_id), Some(canvas)) = (canvases.current_id(), canvases.current()) else {
            return;
        };

        let mut tool_proxies = services.service_mut::<CanvasToolProxies>();
        let tool_proxy = tool_proxies.get_mut(&canvas_id);
        tool_proxy.switch_tool(T::tool(), &canvas);
    }
}
