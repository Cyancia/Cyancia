use std::{marker::PhantomData, sync::Arc, time::Instant};

use cyancia_canvas::CanvasManager;
use cyancia_input::action::{Action, ActionId};
use cyancia_runtime::Services;
use cyancia_tools::{ToolId, ToolProxies};
use iced_runtime::Task;

use crate::ActionFunction;

pub trait CanvasToolAction: Send + Sync + 'static {
    fn action() -> ActionId;
    fn tool() -> ToolId;
}

macro_rules! canvas_tool_action {
    ($name:ident, $action:literal, $tool: literal) => {
        #[derive(Default)]
        pub struct $name;
        impl CanvasToolAction for $name {
            fn action() -> ActionId {
                ActionId::new($action.into())
            }
            fn tool() -> ToolId {
                ToolId::new($tool.into())
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

impl<T: CanvasToolAction> ActionFunction for CanvasToolSwitch<T> {
    type Message = ();

    fn id(&self) -> ActionId {
        T::action()
    }

    fn trigger(&self, services: &mut Services) -> Task<Self::Message> {
        let canvases = services.service::<CanvasManager>();
        let Some(canvas) = canvases.current() else {
            return Task::none();
        };
        let canvas_tool_proxy_id = canvas.tool_proxy_id;
        // Drop the read guard before taking the write guard on ToolProxies.
        drop(canvases);

        let mut tool_proxies = services.remove_service::<ToolProxies>();
        let tool_proxy = tool_proxies.get_mut(&canvas_tool_proxy_id);
        tool_proxy.switch_tool(T::tool(), services);
        services.insert_service(tool_proxies);

        Task::none()
    }
}
