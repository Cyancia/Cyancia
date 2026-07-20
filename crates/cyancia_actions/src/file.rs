use cyancia_canvas::{CCanvas, CanvasAppExt, CanvasManager, event::CanvasActiveLayerChanged};
use cyancia_image::{
    CImage,
    texel::TexelType,
    tile::{GpuLayerInfo, GpuTileStorage},
};
use cyancia_tools::{ToolProxies, ToolProxy};
use cyancia_undo::{UndoStack, UndoStacks};
use cyancia_utils::log_err::LogErr;
use gpui::{App, actions};
use rfd::AsyncFileDialog;

use crate::ActionFunction;

actions!([OpenFileAction, SaveFileAction]);

impl ActionFunction for OpenFileAction {
    fn trigger(&self, cx: &mut App) {
        cx.spawn(async |cx| {
            let Some(file) = AsyncFileDialog::new().pick_file().await else {
                log::error!("Unable to get selected file path.");
                return;
            };

            let Ok((image, archive)) = cx.update(|cx| CImage::from_file(file.path(), cx)) else {
                return;
            };

            log::info!("Opened image from file {:?}.", file);

            let tool_proxy_id = cx.update_global::<ToolProxies, _>(|tool_proxies, _| {
                // Tool switch is handled in canvas dock, which is outside of async environment.
                // TODO No, just do it here.
                tool_proxies.add(ToolProxy::default())
            });
            let canvas = CCanvas::new(file.path().into(), image, archive, tool_proxy_id);
            let canvas_id = canvas.id();

            cx.update_global::<UndoStacks, _>(|undo_stacks, cx| {
                undo_stacks.insert(*canvas.id(), UndoStack::new(*canvas.id(), 200, cx));
            });

            // TODO this should not be done here
            cx.read_global::<GpuTileStorage, _>(|tiles, _| {
                for layer in canvas.image.layer_stack().iter_layers() {
                    tiles.declare_layer(
                        *layer.id(),
                        GpuLayerInfo {
                            // TODO
                            texel_type: TexelType::RGBA8,
                        },
                    );
                }
                tiles.declare_layer(
                    canvas.image.selection_layer(),
                    GpuLayerInfo {
                        // TODO This should change when image depth is not 8 bit
                        texel_type: TexelType::A8,
                    },
                );
            });

            let canvas_entity = cx.update_global::<CanvasManager, _>(|canvas_manager, cx| {
                canvas_manager.add_canvas(canvas, cx);
                canvas_manager.get(&canvas_id).unwrap().upgrade().unwrap()
            });

            canvas_entity.update(cx, |canvas, cx| {
                cx.emit(CanvasActiveLayerChanged {
                    from: canvas.active_layer_id(),
                    to: canvas.active_layer_id(),
                });
            });
        })
        .detach();
    }
}

impl ActionFunction for SaveFileAction {
    fn trigger(&self, cx: &mut App) {
        cx.update_current_canvas(|canvas, cx| {
            if canvas.archive.path().is_none()
                && canvas
                    .set_file_path(canvas.file_path().with_extension("cyan"))
                    .logged_err()
                    .is_err()
            {
                return;
            }

            // TODO nonononono use async
            futures::executor::block_on(canvas.image.write_archive(&canvas.archive, cx)).log_err();
        });
    }
}
