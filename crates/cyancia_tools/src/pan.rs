use cyancia_id::Id;

use crate::{CanvasTool, CanvasToolFunction};

#[derive(Default)]
pub struct PanTool;

impl CanvasToolFunction for PanTool {
    fn id(&self) -> Id<CanvasTool> {
        Id::from_str("pan_tool")
    }
}
