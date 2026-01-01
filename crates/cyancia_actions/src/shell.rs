use std::sync::Arc;

use cyancia_canvas::CCanvas;
use cyancia_id::Id;
use cyancia_tools::{CanvasTool, ToolProxy};

pub struct DestructedShell {
    pub current_canvas: Arc<CCanvas>,
    pub canvases: Vec<Arc<CCanvas>>,
}

pub struct CShell<'a> {
    current_canvas: Arc<CCanvas>,
    canvas_creation: Vec<Arc<CCanvas>>,
    tool_proxy: &'a mut ToolProxy,
}

impl<'a> CShell<'a> {
    pub fn new(current_canvas: Arc<CCanvas>, tool_proxy: &'a mut ToolProxy) -> Self {
        Self {
            current_canvas,
            canvas_creation: Vec::new(),
            tool_proxy,
        }
    }

    pub fn canvas(&self) -> Arc<CCanvas> {
        self.current_canvas.clone()
    }

    // pub fn all_canvases(&self) -> &[Arc<CCanvas>] {
    //     &self.all_canvases
    // }

    pub fn request_canvas_creation(&mut self, canvas: Arc<CCanvas>) {
        // if self.all_canvases.iter().any(|c| Arc::ptr_eq(c, &canvas)) {
        //     self.current_canvas = canvas;
        // } else {
        //     self.canvas_creation.push(canvas);
        // }
        self.canvas_creation.push(canvas);
    }

    pub fn destruct(self) -> DestructedShell {
        DestructedShell {
            current_canvas: self.current_canvas,
            canvases: self.canvas_creation,
        }
    }

    pub fn tool_proxy(&mut self) -> &mut ToolProxy {
        self.tool_proxy
    }
}
