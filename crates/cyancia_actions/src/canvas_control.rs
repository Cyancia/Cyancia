use cyancia_canvas::CanvasAppExt;
use cyancia_tools::{ToolId, ToolProxies};
use gpui::{App, BorrowAppContext, actions};

use crate::ActionFunction;

pub trait CanvasToolAction: Send + Sync + 'static {
    fn tool() -> ToolId;
}

macro_rules! canvas_tool_action {
    ($name:ident, $tool: literal) => {
        actions!([$name]);

        impl ActionFunction for $name {
            fn trigger(&self, cx: &mut App) {
                trigger_tool_switch(ToolId::new($tool.into()), cx);
            }
        }
    };
}
canvas_tool_action!(SwitchToPanToolAction, "pan_tool");
canvas_tool_action!(SwitchToRotateToolAction, "rotate_tool");
canvas_tool_action!(SwitchToZoomToolAction, "zoom_tool");
canvas_tool_action!(SwitchToBrushToolAction, "brush_tool");
canvas_tool_action!(SwitchToBucketToolAction, "bucket_tool");

fn trigger_tool_switch(tool_id: ToolId, cx: &mut App) {
    let Some(canvas) = cx.read_current_canvas() else {
        return;
    };
    let canvas_tool_proxy_id = canvas.tool_proxy_id();

    cx.update_global::<ToolProxies, _>(|tool_proxies, cx| {
        let tool_proxy = tool_proxies.get_mut(&canvas_tool_proxy_id);
        tool_proxy.switch_tool(tool_id, cx);
    });
}
