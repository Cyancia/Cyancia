use cyancia_canvas::{CanvasAppExt, CanvasUndoStackAppExt, command::TileReplaceCommand};
use cyancia_image::tile::GpuTileStorage;
use cyancia_render::render_context::RenderContext;
use cyancia_utils::log_err::LogErr;
use gpui::{App, actions};

use crate::ActionFunction;

actions!([DeleteSelectionAction]);

impl ActionFunction for DeleteSelectionAction {
    fn trigger(&self, cx: &mut App) {
        let Some(canvas) = cx.read_current_canvas() else {
            return;
        };

        let tiles = cx.global::<GpuTileStorage>();
        let render_context = cx.global::<RenderContext>();
        let selection_layer_id = canvas.image.selection_layer();
        let selection_layer = tiles.get_layer(selection_layer_id).unwrap();

        let cmd = TileReplaceCommand::new_clear(
            "Delete Selection".into(),
            canvas.id(),
            &render_context.device,
            &render_context.queue,
            selection_layer_id,
            &selection_layer,
        );

        drop(selection_layer);

        cx.push_undo_command_to_current(cmd).log_err();
    }
}
