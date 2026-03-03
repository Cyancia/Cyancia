use std::sync::Arc;

use async_trait::async_trait;
use cyancia_canvas::{
    CCanvas, CanvasId, CanvasManager,
    render::{CanvasRenderer, CanvasRenderers},
};
use cyancia_image::{CImage, layer::Layer, tile::GpuTileStorage};
use cyancia_input::{
    action::{Action, ActionId},
    key::KeySequence,
};
use cyancia_runtime::{Services, service::FromRuntime};
use cyancia_tools::{CanvasToolFunctionRegistry, CanvasToolProxies};
use glam::UVec2;
use iced_runtime::Task;
use rfd::{AsyncFileDialog, FileDialog};
use uuid::Uuid;

use crate::ActionFunction;

#[derive(Default)]
pub struct OpenFileAction {}

#[async_trait]
impl ActionFunction for OpenFileAction {
    fn id(&self) -> ActionId {
        ActionId::new("open_file_action".into())
    }

    async fn trigger(&self, services: Arc<Services>) {
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
        let layer = Layer::from_image(img, services.service::<GpuTileStorage>().as_ref());
        let canvas = CCanvas {
            id: CanvasId::new(Uuid::new_v4()),
            image: Arc::new(CImage::from_layer(UVec2::new(width, height), layer)),
            transform: Default::default(),
        };

        services.service_mut::<CanvasToolProxies>().add(
            &canvas.id,
            &services.service::<CanvasToolFunctionRegistry>(),
        );
        services
            .service_mut::<CanvasRenderers>()
            .insert(canvas.id, CanvasRenderer::from_runtime(&services));
        services.service_mut::<CanvasManager>().add_canvas(canvas);
    }
}
