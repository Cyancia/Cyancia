use cyancia_canvas::{CCanvas, CanvasManager, event::CanvasActiveLayerChanged};
use cyancia_image::{
    CImage,
    blend_modes::BlendMode,
    composite::BlendFunction,
    layer::LayerData,
    texel::TexelType,
    tile::{GpuLayerInfo, GpuTileStorage},
};
use cyancia_tools::{ToolProxies, ToolProxy};
use cyancia_undo::{UndoStack, UndoStacks};
use glam::UVec2;
use gpui::{App, actions};
use rfd::AsyncFileDialog;

use crate::ActionFunction;

actions!([OpenFileAction]);

impl ActionFunction for OpenFileAction {
    fn trigger(&self, cx: &mut App) {
        cx.spawn(async |cx| {
            let Some(file) = AsyncFileDialog::new().pick_file().await else {
                log::error!("Unable to get selected file path.");
                return;
            };

            let img = match image::load_from_memory(&file.read().await) {
                Ok(i) => i,
                Err(e) => {
                    log::error!("Unable to open image from file {:?}: {}", file, e);
                    return;
                }
            };
            log::info!("Opened image from file {:?}.", file);

            let width = img.width();
            let height = img.height();
            let layer = cx.read_global::<GpuTileStorage, _>(|tiles, _| {
                LayerData::from_image("Background".into(), img, tiles, BlendMode::Normal.id())
            });

            let image = CImage::from_layer(UVec2::new(width, height), layer);

            let tool_proxy_id = cx.update_global::<ToolProxies, _>(|tool_proxies, _| {
                // Tool switch is handled in canvas dock, which is outside of async environment.
                tool_proxies.add(ToolProxy::default())
            });
            let canvas = CCanvas::new(image, tool_proxy_id);
            let canvas_id = canvas.id();

            cx.update_global::<UndoStacks, _>(|undo_stacks, _| {
                undo_stacks.insert(*canvas.id(), UndoStack::new(200))
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
