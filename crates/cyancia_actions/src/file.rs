use std::sync::Arc;

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
use cyancia_input::action::ActionId;
use cyancia_runtime::{Services, event::Event, service::FromServices};
use cyancia_tools::{ToolId, ToolProxies, ToolProxy};
use glam::UVec2;
use iced_runtime::Task;
use rfd::AsyncFileDialog;
use uuid::Uuid;

use crate::ActionFunction;

#[derive(Default)]
pub struct OpenFileAction {}

pub enum OpenFileMessage {
    ImageCreated(CImage),
    Noop,
}

async fn open_file(tiles: GpuTileStorage) -> OpenFileMessage {
    let Some(file) = AsyncFileDialog::new().pick_file().await else {
        log::error!("Unable to get selected file path.");
        return OpenFileMessage::Noop;
    };

    let img = match image::load_from_memory(&file.read().await) {
        Ok(i) => i,
        Err(e) => {
            log::error!("Unable to open image from file {:?}: {}", file, e);
            return OpenFileMessage::Noop;
        }
    };
    log::info!("Opened image from file {:?}.", file);

    let width = img.width();
    let height = img.height();
    let layer = LayerData::from_image(
        "Background".into(),
        img,
        &tiles,
        Box::new(BlendMode::Normal),
    );

    OpenFileMessage::ImageCreated(CImage::from_layer(UVec2::new(width, height), layer))
}

impl ActionFunction for OpenFileAction {
    type Message = OpenFileMessage;

    fn id(&self) -> ActionId {
        ActionId::new("open_file_action".into())
    }

    fn trigger(&self, services: &mut Services) -> Task<Self::Message> {
        let tiles = services.service::<GpuTileStorage>().clone();

        Task::future(open_file(tiles))
    }

    fn handle_message(
        &self,
        message: Self::Message,
        services: &mut Services,
    ) -> Task<Self::Message> {
        match message {
            OpenFileMessage::ImageCreated(image) => {
                let mut tool_proxy = ToolProxy::new();
                tool_proxy.switch_tool(ToolId::new("pan_tool".into()), services);
                let tool_proxy_id = services.service_mut::<ToolProxies>().add(tool_proxy);
                let canvas = CCanvas::new(image, tool_proxy_id);

                // TODO this should not be done here
                let tiles = services.service::<GpuTileStorage>();
                for layer in canvas.image.layer_stack().iter_layers() {
                    tiles.declare_layer(
                        layer.id(),
                        GpuLayerInfo {
                            // TODO
                            texel_type: TexelType::RGBA8,
                        },
                    );
                }
                let id = canvas.id();
                services.service_mut::<CanvasManager>().add_canvas(canvas);

                CanvasCreated::broadcast(CanvasCreated { id });

                Task::none()
            }
            OpenFileMessage::Noop => Task::none(),
        }
    }
}
