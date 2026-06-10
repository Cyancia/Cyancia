use cyancia_canvas::CanvasAppExt;
use cyancia_image::tile::{GpuTileStorage, GpuTileStorageInner};
use cyancia_render::render_context::RenderContext;
use cyancia_tools::{ToolFunction, ToolId, ToolsAppExt};
use glam::Vec2;
use gpui::{App, Context, MouseUpEvent};

use crate::bucket::{Bucket, BucketParams};

pub mod bucket;

pub fn init(cx: &mut App) {
    cx.add_tool_function::<BucketTool>();
}

const _: () = {
    if GpuTileStorageInner::TILE_SIZE % 32 != 0 {
        panic!(
            "Tile size must be divisible by 32, otherwise computations in shaders will be incorrect"
        );
    }
};

#[derive(Default)]
pub struct BucketTool {}

impl ToolFunction for BucketTool {
    fn new(cx: &mut Context<Self>) -> Self {
        Self::default()
    }

    fn id() -> ToolId {
        ToolId::new("bucket_tool")
    }

    fn end(&mut self, mouse: &MouseUpEvent, cx: &mut Context<Self>) {
        let Some(canvas) = cx.read_current_canvas() else {
            return;
        };

        let position_ws = Vec2::new(mouse.position.x.into(), mouse.position.y.into());
        let Some(position_ps) = canvas.transform.window_to_pixel(position_ws) else {
            return;
        };
        if position_ps.x < 0.0
            || position_ps.y < 0.0
            || position_ps.x > canvas.image.size().x as f32
            || position_ps.y > canvas.image.size().y as f32
        {
            return;
        }

        let tiles = cx.global::<GpuTileStorage>();
        let render_context = cx.global::<RenderContext>();
        let ref_layer_id = canvas.image.active_layer;
        let ref_layer = tiles.get_layer_binding_or_empty(ref_layer_id).unwrap();
        let ref_layer_info = tiles.get_layer_info(ref_layer_id).unwrap();

        let params = BucketParams {
            seed: position_ps.as_uvec2(),
            // TODO
            threshold: 0.05,
            alpha_threshold: 0.02,
        };

        let bucket = Bucket::new(&render_context.device, ref_layer_info.texel_type);
        let prepared = bucket.prepare(
            &render_context.device,
            &render_context.queue,
            &params,
            &ref_layer,
        );
        bucket.dispatch(&render_context.device, &render_context.queue, prepared);
    }
}
