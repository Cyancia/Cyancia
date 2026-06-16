use cyancia_canvas::CanvasAppExt;
use cyancia_image::{
    layer::{LayerData, LayerStackNode},
    texel::TexelType,
    tile::{GpuLayerInfo, GpuTileStorage},
};
use gpui::{App, actions};

use crate::ActionFunction;

actions!([
    CreateNewLayerAction,
    GroupActiveLayerAction,
    MoveLayerUpAction,
    MoveLayerDownAction
]);

impl ActionFunction for CreateNewLayerAction {
    fn trigger(&self, cx: &mut App) {
        cx.update_current_canvas(|canvas, cx| {
            // TODO: Check if this type of layer can be created under the current layer.
            //       If can't, check it's parent, until find one.
            let parent = canvas.image.parent_of_active_layer();
            let active_layer_id = canvas.image.active_layer;

            let new_layer =
                LayerData::new_normal_pixel(canvas.image.next_name_of_layer("Layer".into()));
            let new_layer_id = new_layer.id();
            let parent_node = canvas
                .image
                .layer_stack_mut()
                .find_node_mut(parent)
                .expect("Parent of active layer should always exist");
            parent_node.insert_child_above(active_layer_id, LayerStackNode::new(new_layer.id()));
            canvas.image.active_layer = new_layer_id;
            canvas
                .image
                .layer_stack_mut()
                .insert_isolated_layer(new_layer);

            cx.refresh_windows();

            let tiles = cx.global::<GpuTileStorage>();
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
        });
    }
}

impl ActionFunction for GroupActiveLayerAction {
    fn trigger(&self, cx: &mut App) {
        cx.update_current_canvas(|canvas, _| {
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
        });
    }
}

impl ActionFunction for MoveLayerUpAction {
    fn trigger(&self, cx: &mut App) {
        cx.update_current_canvas(|canvas, _| {
            let active_layer_id = canvas.image.active_layer;
            let active_layer_parent = canvas.image.parent_of_active_layer();
            let active_layer_parent_node = canvas
                .image
                .layer_stack()
                .find_node(active_layer_parent)
                .expect("Parent of active layer should always exist");
            let active_layer_index = active_layer_parent_node
                .children()
                .iter()
                .position(|child| child.id() == active_layer_id)
                .expect("Active layer should always be a child of its parent");

            if let Some(sibling_id) = active_layer_parent_node
                .child_above(active_layer_id)
                .map(|s| s.id())
            {
                // Parent node has a sibling. So the node won't go out of parent node.
                if canvas
                    .image
                    .layer_stack()
                    .can_have_children_of(sibling_id, active_layer_id)
                    .expect("Sibling layer should always exist")
                {
                    // Sibling node can have active layer as children, so move active layer into sibling node.
                    let active_layer_parent_node = canvas
                        .image
                        .layer_stack_mut()
                        .find_node_mut(active_layer_parent)
                        .expect("Parent of active layer should always exist");
                    let active_layer_node = active_layer_parent_node
                        .remove_child_at(active_layer_index)
                        .expect("Active layer should always be a child of its parent");
                    let sibling_node = canvas
                        .image
                        .layer_stack_mut()
                        .find_node_mut(sibling_id)
                        .expect("Sibling layer should always exist");
                    sibling_node.insert_background_child(active_layer_node);
                } else {
                    // If can't, swap them.
                    let active_layer_parent_node = canvas
                        .image
                        .layer_stack_mut()
                        .find_node_mut(active_layer_parent)
                        .expect("Parent of active layer should always exist");
                    active_layer_parent_node.swap_children(active_layer_id, sibling_id);
                }
            } else if let Some(active_layer_parent_parent) = active_layer_parent_node.parent() {
                // Active node is the last child, so we are moving it out of its parent.
                let active_layer_parent_node = canvas
                    .image
                    .layer_stack_mut()
                    .find_node_mut(active_layer_parent)
                    .expect("Parent of active layer should always exist");
                let active_layer_node = active_layer_parent_node
                    .remove_child_at(active_layer_index)
                    .expect("Active layer should always be a child of its parent");
                let active_layer_parent_parent_node = canvas
                    .image
                    .layer_stack_mut()
                    .find_node_mut(active_layer_parent_parent)
                    .expect("Parent of parent of active layer should always exist");
                active_layer_parent_parent_node
                    .insert_child_above(active_layer_parent, active_layer_node);
            }
        });
    }
}

impl ActionFunction for MoveLayerDownAction {
    fn trigger(&self, cx: &mut App) {
        cx.update_current_canvas(|canvas, _| {
            let active_layer_id = canvas.image.active_layer;
            let active_layer_parent = canvas.image.parent_of_active_layer();
            let active_layer_parent_node = canvas
                .image
                .layer_stack()
                .find_node(active_layer_parent)
                .expect("Parent of active layer should always exist");
            let active_layer_index = active_layer_parent_node
                .children()
                .iter()
                .position(|child| child.id() == active_layer_id)
                .expect("Active layer should always be a child of its parent");

            if let Some(sibling_id) = active_layer_parent_node
                .child_below(active_layer_id)
                .map(|s| s.id())
            {
                // Parent node has a sibling. So the node won't go out of parent node.
                if canvas
                    .image
                    .layer_stack()
                    .can_have_children_of(sibling_id, active_layer_id)
                    .expect("Sibling layer should always exist")
                {
                    // Sibling node can have active layer as children, so move active layer into sibling node.
                    let active_layer_parent_node = canvas
                        .image
                        .layer_stack_mut()
                        .find_node_mut(active_layer_parent)
                        .expect("Parent of active layer should always exist");
                    let active_layer_node = active_layer_parent_node
                        .remove_child_at(active_layer_index)
                        .expect("Active layer should always be a child of its parent");
                    let sibling_node = canvas
                        .image
                        .layer_stack_mut()
                        .find_node_mut(sibling_id)
                        .expect("Sibling layer should always exist");
                    sibling_node.insert_foreground_child(active_layer_node);
                } else {
                    // If can't, swap them.
                    let active_layer_parent_node = canvas
                        .image
                        .layer_stack_mut()
                        .find_node_mut(active_layer_parent)
                        .expect("Parent of active layer should always exist");
                    active_layer_parent_node.swap_children(active_layer_id, sibling_id);
                }
            } else if let Some(active_layer_parent_parent) = active_layer_parent_node.parent() {
                // Active node is the first child, so we are moving it out of its parent.
                let active_layer_parent_node = canvas
                    .image
                    .layer_stack_mut()
                    .find_node_mut(active_layer_parent)
                    .expect("Parent of active layer should always exist");
                let active_layer_node = active_layer_parent_node
                    .remove_child_at(active_layer_index)
                    .expect("Active layer should always be a child of its parent");
                let active_layer_parent_parent_node = canvas
                    .image
                    .layer_stack_mut()
                    .find_node_mut(active_layer_parent_parent)
                    .expect("Parent of parent of active layer should always exist");
                active_layer_parent_parent_node
                    .insert_child_below(active_layer_parent, active_layer_node);
            }
        });
    }
}
