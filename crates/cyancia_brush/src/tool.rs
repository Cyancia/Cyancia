use cyancia_assets::store::AssetRegistry;
use cyancia_canvas::{CCanvas, CanvasManager};
use cyancia_image::tile::GpuTileStorage;
use cyancia_input::{key::KeyboardState, mouse::PressedMouseState};
use cyancia_runtime::{Services, service::Service};
use cyancia_tools::{ToolFunction, ToolId};
use cyancia_utils::wrapper;
use glam::Vec2;

use crate::{
    asset::BrushPresetInstance,
    render::{BrushPresetOperator, graph::GraphInputParams},
};

#[derive(Default)]
pub struct BrushTool;

impl ToolFunction for BrushTool {
    fn id(&self) -> ToolId {
        ToolId::new("brush_tool".into())
    }

    fn update(&mut self, keyboard: &KeyboardState, mouse: &PressedMouseState, services: &Services) {
        let Some(canvas) = services.service::<CanvasManager>().current() else {
            return;
        };
        let Some(mut brush) = services.get_service_mut::<CurrentBrushPresetOperator>() else {
            log::error!("No current brush preset operator found.");
            return;
        };

        let params = GraphInputParams {
            pen_position: canvas
                .transform
                .read()
                .pixel_to_widget
                .inverse()
                .transform_point2(Vec2::new(mouse.position.x, mouse.position.y)),
        };
        brush.prepare(
            params,
            canvas.image.root().id(),
            services.service::<GpuTileStorage>().as_ref(),
            services.service::<AssetRegistry>().as_ref(),
        );
        brush.draw();
    }
}

wrapper! {
    pub mut CurrentBrushPresetOperator : BrushPresetOperator
}

impl Service for CurrentBrushPresetOperator {}
