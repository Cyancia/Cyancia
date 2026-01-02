use std::{marker::PhantomData, time::Instant};

use cyancia_id::Id;
use cyancia_input::action::Action;
use cyancia_tools::CanvasTool;

use crate::{ActionFunction, shell::ActionShell};

pub trait CanvasToolAction: Send + Sync + 'static {
    fn action() -> Id<Action>;
    fn tool() -> Id<CanvasTool>;
}

macro_rules! canvas_tool_action {
    ($name:ident, $action:literal, $tool: literal) => {
        pub struct $name;
        impl CanvasToolAction for $name {
            fn action() -> Id<Action> {
                Id::from_str($action)
            }
            fn tool() -> Id<CanvasTool> {
                Id::from_str($tool)
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
    fn id(&self) -> Id<Action> {
        T::action()
    }

    fn trigger(&self, shell: &mut ActionShell) {
        let canvas = shell.canvas();
        shell.tool_proxy().switch_tool(T::tool(), &canvas);
    }
}
