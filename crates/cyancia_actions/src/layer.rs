use std::sync::Arc;

use cyancia_canvas::CanvasManager;
use cyancia_image::{
    layer::Layer,
    texel::TexelType,
    tile::{GpuLayerInfo, GpuTileStorage},
};
use cyancia_input::action::ActionId;
use cyancia_runtime::Services;
use iced_runtime::Task;

use crate::ActionFunction;

#[derive(Default)]
pub struct CreateNewLayerAction;

impl ActionFunction for CreateNewLayerAction {
    type Message = ();

    fn id(&self) -> ActionId {
        ActionId::new("create_new_layer_action".into())
    }

    fn trigger(&self, services: &mut Services) -> Task<()> {
        let Some(canvas) = services.service_mut::<CanvasManager>().current_mut() else {
            return Task::none();
        };

        // TODO: Check if this type of layer can be created under the current layer.
        //       If can't, check it's parent, until find one.
        let Some(parent) = canvas.image.parent_of_active_layer() else {
            return Task::none();
        };

        let new_layer = canvas.image.create_new_layer("Layer".into(), parent);
        let tiles = services.service::<GpuTileStorage>();
        tiles.declare_layer(
            new_layer,
            GpuLayerInfo {
                // TODO use image format
                texel_type: TexelType::RGBA8,
            },
        );
        log::info!(
            "Created new layer with id {:?} under parent {:?}.",
            new_layer,
            parent
        );

        Task::none()
    }
}
