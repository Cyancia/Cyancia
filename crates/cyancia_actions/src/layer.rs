use cyancia_canvas::{
    CanvasAppExt, CanvasUndoStackAppExt,
    command::{GroupLayerCommand, GroupedLayer, InsertLayerCommand, MoveLayerCommand},
};
use cyancia_image::{
    layer::LayerData,
    texel::TexelType,
    tile::{GpuLayerInfo, TileStorageAppExt},
};
use cyancia_utils::log_err::LogErr;
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
        let cmd = cx
            .update_current_canvas(|canvas, _| {
                // TODO: Check if this type of layer can be created under the current layer.
                //       If can't, check it's parent, until find one.
                let parent = canvas.image.parent_of_active_layer();
                let active_layer_id = canvas.image.active_layer;

                let new_layer =
                    LayerData::new_normal_pixel(canvas.image.next_name_of_layer("Layer".into()));
                let parent_node = canvas.image.layer_stack().find_node(parent).unwrap();
                let index = parent_node.child_index(active_layer_id).unwrap();

                InsertLayerCommand {
                    canvas: canvas.id(),
                    layer: new_layer,
                    parent_id: parent,
                    index: index + 1,
                    previous_active_layer: active_layer_id,
                }
            })
            .unwrap();

        let new_layer_id = cmd.layer.id();

        if cx.push_undo_command_to_current(cmd).logged_err().is_err() {
            return;
        }

        let tiles = cx.tile_storage();
        tiles.declare_layer(
            new_layer_id,
            GpuLayerInfo {
                // TODO use image format
                texel_type: TexelType::RGBA8,
            },
        );
    }
}

impl ActionFunction for GroupActiveLayerAction {
    fn trigger(&self, cx: &mut App) {
        let cmd = cx
            .update_current_canvas(|canvas, _| {
                // TODO Support grouping multiple selected layers.
                let group_name = canvas.image.next_name_of_layer("Group".to_string());
                let active_layer_id = canvas.image.active_layer;
                let active_layer_parent = canvas.image.parent_of_active_layer();
                let parent = canvas
                    .image
                    .layer_stack()
                    .find_node(active_layer_parent)
                    .unwrap();
                let active_layer_index = parent.child_index(active_layer_id).unwrap();

                let group_layer = LayerData::new_normal_group(group_name);

                GroupLayerCommand {
                    canvas: canvas.id(),
                    group: group_layer,
                    children: vec![GroupedLayer {
                        id: active_layer_id,
                        original_parent: active_layer_parent,
                        original_index: active_layer_index,
                    }],
                    parent_id: active_layer_parent,
                    index: active_layer_index,
                    previous_active_layer: active_layer_id,
                }
            })
            .unwrap();

        cx.push_undo_command_to_current(cmd).log_err();
    }
}

impl ActionFunction for MoveLayerUpAction {
    fn trigger(&self, cx: &mut App) {
        let Some(canvas) = cx.read_current_canvas() else {
            return;
        };
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

        let (new_parent, new_index) = if let Some(sibling_id) = active_layer_parent_node
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
                (sibling_id, 0)
            } else {
                // If can't, swap them.
                let active_layer_parent_node = canvas
                    .image
                    .layer_stack()
                    .find_node(active_layer_parent)
                    .expect("Parent of active layer should always exist");
                (
                    active_layer_parent,
                    active_layer_parent_node.child_index(sibling_id).unwrap(),
                )
            }
        } else if let Some(active_layer_parent_parent) = active_layer_parent_node.parent() {
            // Active node is the last child, so we are moving it out of its parent.
            let active_layer_parent_parent_node = canvas
                .image
                .layer_stack()
                .find_node(active_layer_parent_parent)
                .expect("Parent of parent of active layer should always exist");
            (
                active_layer_parent_parent,
                active_layer_parent_parent_node
                    .child_index(active_layer_parent)
                    .unwrap()
                    + 1,
            )
        } else {
            return;
        };

        cx.push_undo_command_to_current(MoveLayerCommand {
            canvas: canvas.id(),
            layer: active_layer_id,
            original_parent: active_layer_parent,
            original_index: active_layer_index,
            new_parent,
            new_index,
        })
        .log_err();
    }
}

impl ActionFunction for MoveLayerDownAction {
    fn trigger(&self, cx: &mut App) {
        let Some(canvas) = cx.read_current_canvas() else {
            return;
        };

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

        let (new_parent, new_index) = if let Some(sibling_id) = active_layer_parent_node
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
                let sibling_node = canvas
                    .image
                    .layer_stack()
                    .find_node(sibling_id)
                    .expect("Sibling layer should always exist");
                (sibling_id, sibling_node.n_children())
            } else {
                // If can't, swap them.
                let active_layer_parent_node = canvas
                    .image
                    .layer_stack()
                    .find_node(active_layer_parent)
                    .expect("Parent of active layer should always exist");
                (
                    active_layer_parent,
                    active_layer_parent_node.child_index(sibling_id).unwrap(),
                )
            }
        } else if let Some(active_layer_parent_parent) = active_layer_parent_node.parent() {
            // Active node is the first child, so we are moving it out of its parent.
            let active_layer_parent_parent_node = canvas
                .image
                .layer_stack()
                .find_node(active_layer_parent_parent)
                .expect("Parent of parent of active layer should always exist");
            (
                active_layer_parent_parent,
                active_layer_parent_parent_node
                    .child_index(active_layer_parent)
                    .unwrap(),
            )
        } else {
            return;
        };

        cx.push_undo_command_to_current(MoveLayerCommand {
            canvas: canvas.id(),
            layer: active_layer_id,
            original_parent: active_layer_parent,
            original_index: active_layer_index,
            new_parent,
            new_index,
        })
        .log_err();
    }
}
