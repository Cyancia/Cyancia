use std::sync::Arc;

use anyhow::bail;
use cyancia_canvas::{
    CCanvas, CanvasId, CanvasManager, event::CanvasCreated, render::CanvasRenderer,
};
use cyancia_image::{
    CImage,
    blend_modes::BlendMode,
    layer::LayerData,
    texel::TexelType,
    tile::{GpuLayerInfo, GpuTileStorage},
};
use cyancia_tools::{ToolId, ToolProxies, ToolProxy};
use glam::UVec2;
use gpui::{Action, App, AppContext, actions};
use rfd::AsyncFileDialog;
use schemars::JsonSchema;
use serde::Serialize;
use uuid::Uuid;

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
            let layer = cx.read_global::<GpuTileStorage, _>(|tiles, cx| {
                LayerData::from_image("Background".into(), img, tiles, Box::new(BlendMode::Normal))
            });

            let image = CImage::from_layer(UVec2::new(width, height), layer);

            let tool_proxy_id = cx.update_global::<ToolProxies, _>(|tool_proxies, cx| {
                // Tool switch is handled in canvas dock, which is outside of async environment.
                tool_proxies.add(ToolProxy::new())
            });
            let canvas = CCanvas::new(image, tool_proxy_id);

            // TODO this should not be done here
            cx.read_global::<GpuTileStorage, _>(|tiles, cx| {
                for layer in canvas.image.layer_stack().iter_layers() {
                    tiles.declare_layer(
                        layer.id(),
                        GpuLayerInfo {
                            // TODO
                            texel_type: TexelType::RGBA8,
                        },
                    );
                }
            });

            cx.update_global::<CanvasManager, _>(|canvas_manager, cx| {
                canvas_manager.add_canvas(canvas, cx);
            });
        })
        .detach();
    }
}
