use std::sync::Arc;

use cyancia_canvas::{
    CCanvas, CanvasId, CanvasManager,
    render::{CanvasRenderer, CanvasRenderers},
};
use cyancia_image::{
    CImage,
    layer::Layer,
    texel::TexelType,
    tile::{GpuLayerInfo, GpuTileStorage},
};
use cyancia_input::action::ActionId;
use cyancia_runtime::{Services, service::FromRuntime};
use cyancia_tools::{ToolId, ToolProxies, ToolProxy};
use glam::UVec2;
use iced_runtime::Task;
use rfd::AsyncFileDialog;
use uuid::Uuid;

use crate::ActionFunction;

#[derive(Default)]
pub struct OpenFileAction {}

impl ActionFunction for OpenFileAction {
    fn id(&self) -> ActionId {
        ActionId::new("open_file_action".into())
    }

    fn trigger(&self, services: Arc<Services>) -> Task<()> {
        let task = Task::future(async move {
            let Some(file) = AsyncFileDialog::new().pick_file().await else {
                log::error!("Unable to get selected file path.");
                return Task::none();
            };

            let img = match image::load_from_memory(&file.read().await) {
                Ok(i) => i,
                Err(e) => {
                    log::error!("Unable to open image from file {:?}: {}", file, e);
                    return Task::none();
                }
            };
            log::info!("Opened image from file {:?}.", file);

            let width = img.width();
            let height = img.height();
            let layer = Layer::from_image(img, services.service::<GpuTileStorage>().as_ref());
            let mut tool_proxy = ToolProxy::new();
            let initial_tool_task =
                tool_proxy.switch_tool(ToolId::new("pan_tool".into()), services.clone());
            let tool_proxy_id = services.service_mut::<ToolProxies>().add(tool_proxy);
            let canvas = CCanvas {
                id: CanvasId::new(Uuid::new_v4()),
                tool_proxy_id,
                image: Arc::new(CImage::from_layer(UVec2::new(width, height), layer)),
                transform: Default::default(),
            };

            services
                .service_mut::<CanvasRenderers>()
                .insert(canvas.id, CanvasRenderer::from_runtime(&services));
            // TODO this should not be done here
            services.service::<GpuTileStorage>().declare_layer(
                canvas.image.root().id(),
                GpuLayerInfo {
                    texel_type: TexelType::RGBA8,
                },
            );
            services.service_mut::<CanvasManager>().add_canvas(canvas);

            initial_tool_task
        });

        task.then(|t| t)
    }
}
