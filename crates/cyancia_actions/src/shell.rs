use std::sync::Arc;

use cyancia_canvas::CCanvas;
use cyancia_id::Id;
use cyancia_tools::{CanvasTool, ToolProxy};
use iced_runtime::Task;

use crate::task::ActionTask;

pub struct DestructedShell {
    pub current_canvas: Arc<CCanvas>,
    pub tasks: Vec<Task<Box<dyn ActionTask>>>,
}

pub struct ActionShell {
    current_canvas: Arc<CCanvas>,
    tool_proxy: Arc<ToolProxy>,
    tasks: Vec<Task<Box<dyn ActionTask>>>,
}

impl ActionShell {
    pub fn new(current_canvas: Arc<CCanvas>, tool_proxy: Arc<ToolProxy>) -> Self {
        Self {
            current_canvas,
            tool_proxy,
            tasks: Vec::new(),
        }
    }

    pub fn canvas(&self) -> Arc<CCanvas> {
        self.current_canvas.clone()
    }

    // pub fn all_canvases(&self) -> &[Arc<CCanvas>] {
    //     &self.all_canvases
    // }

    pub fn set_current_canvas(&mut self, canvas: Arc<CCanvas>) {
        // if self.all_canvases.iter().any(|c| Arc::ptr_eq(c, &canvas)) {
        //     self.current_canvas = canvas;
        // } else {
        //     self.canvas_creation.push(canvas);
        // }
        self.current_canvas = canvas;
    }

    pub fn destruct(self) -> DestructedShell {
        DestructedShell {
            current_canvas: self.current_canvas,
            // canvases: self.canvas_creation,
            tasks: self.tasks,
        }
    }

    pub fn tool_proxy(&mut self) -> Arc<ToolProxy> {
        self.tool_proxy.clone()
    }

    pub fn queue_task<T: ActionTask>(&mut self, task: Task<T>) {
        self.tasks
            .push(task.map(|t| Box::new(t) as Box<dyn ActionTask>));
    }
}
