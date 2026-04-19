use std::sync::Arc;

use cyancia_canvas::CanvasManager;
use cyancia_image::{
    layer::{LayerData, LayerStackNode},
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
        let parent = canvas.image.parent_of_active_layer();

        let new_layer =
            LayerData::new_normal_pixel(canvas.image.next_name_of_layer("Layer".into()));
        let new_layer_id = new_layer.id();
        canvas.image.insert_new_layer(parent, new_layer);
        canvas.image.active_layer = new_layer_id;

        let tiles = services.service::<GpuTileStorage>();
        tiles.declare_layer(
            new_layer_id,
            GpuLayerInfo {
                // TODO use image format
                texel_type: TexelType::RGBA8,
            },
        );
        log::info!(
            "Created new layer with id {:?} under parent {:?}.",
            new_layer_id,
            parent
        );

        Task::none()
    }
}

#[derive(Default)]
pub struct GroupActiveLayerAction;

impl ActionFunction for GroupActiveLayerAction {
    type Message = ();

    fn id(&self) -> ActionId {
        ActionId::new("group_active_layer_action".into())
    }

    fn trigger(&self, services: &mut Services) -> Task<()> {
        let Some(canvas) = services.service_mut::<CanvasManager>().current_mut() else {
            return Task::none();
        };

        let group_name = canvas.image.next_name_of_layer("Group".to_string());
        let active_layer_id = canvas.image.active_layer;
        let active_layer_parent = canvas.image.parent_of_active_layer();
        let parent = canvas
            .image
            .layer_stack_mut()
            .find_node_mut(active_layer_parent)
            .expect("Parent of active layer should always exist");
        let active_layer_index = parent
            .children()
            .iter()
            .position(|child| child.id() == active_layer_id)
            .expect("Active layer should always be a child of its parent");
        let active_layer_node = parent
            .remove_child_at(active_layer_index)
            .expect("Active layer should always be a child of its parent");

        let group_layer = LayerData::new_normal_group(group_name);
        let mut group_layer_node = LayerStackNode::new(group_layer.id());
        group_layer_node.insert_background_child(active_layer_node);

        parent.insert_child(active_layer_index, group_layer_node);
        canvas
            .image
            .layer_stack_mut()
            .insert_isolated_layer(group_layer);

        Task::none()
    }
}
