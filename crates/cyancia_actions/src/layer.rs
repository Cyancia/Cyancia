use cyancia_canvas::{
    CanvasAppExt, CanvasUndoStackAppExt,
    command::{
        DeleteLayersCommand, GroupLayerCommand, InsertLayerCommand, LayerWithPosition,
        MoveLayersCommand,
    },
};
use cyancia_image::{
    layer::{LayerData, LayerPosition},
    texel::TexelType,
    tile::{GpuLayerInfo, TileStorageAppExt},
};
use cyancia_utils::log_err::LogErr;
use gpui::{App, actions};

use crate::ActionFunction;

actions!([
    CreateNewLayerAction,
    GroupSelectedLayersAction,
    MoveLayerUpAction,
    MoveLayerDownAction,
    DeleteSelectedLayersAction,
    SelectPreviousLayerAction,
    SelectNextLayerAction,
]);

impl ActionFunction for CreateNewLayerAction {
    fn trigger(&self, cx: &mut App) {
        let cmd = cx
            .update_current_canvas(|canvas, _| {
                // TODO: Check if this type of layer can be created under the current layer.
                //       If can't, check it's parent, until find one.
                let parent = canvas.parent_id_of_active_layer();
                let active_layer_id = canvas.active_layer_id();

                let new_layer =
                    LayerData::new_normal_pixel(canvas.image.next_name_of_layer("Layer".into()));
                let parent_node = canvas.image.layer_stack().get_layer(&parent).unwrap();
                let index = parent_node.child_index(&active_layer_id).unwrap();

                InsertLayerCommand {
                    canvas: canvas.id(),
                    layer: new_layer,
                    parent_id: parent,
                    index: index + 1,
                    previous_active_layer: active_layer_id,
                    previous_selected_layers: canvas.selected_layer_ids().clone(),
                }
            })
            .unwrap();

        let new_layer_id = *cmd.layer.id();

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

impl ActionFunction for GroupSelectedLayersAction {
    fn trigger(&self, cx: &mut App) {
        let cmd = cx
            .update_current_canvas(|canvas, _| {
                let group_name = canvas.image.next_name_of_layer("Group".to_string());
                let reduced_layers = canvas
                    .image
                    .layer_stack()
                    .reduce_ancestors(canvas.selected_layer_ids().iter().copied());
                let sorted_selected_layers = canvas
                    .image
                    .layer_stack()
                    .sort_by_depth_and_index(reduced_layers)
                    .unwrap();
                let children_layers = sorted_selected_layers
                    .into_iter()
                    .map(|l| {
                        let parent = canvas.image.layer_stack().get_parent_of(&l).unwrap();
                        let above = parent.child_below(&l);
                        LayerWithPosition {
                            id: l,
                            original_parent: *parent.id(),
                            original_above: above,
                        }
                    })
                    .collect();

                let (active_layer_parent, active_layer_index) = canvas
                    .image
                    .layer_stack()
                    .get_position_of(&canvas.active_layer_id())
                    .unwrap();

                let group_layer = LayerData::new_normal_group(group_name);

                GroupLayerCommand {
                    canvas: canvas.id(),
                    group: group_layer,
                    children: children_layers,
                    parent_id: *active_layer_parent.id(),
                    index: active_layer_index,
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

        let mut layers = canvas
            .selected_layer_ids()
            .iter()
            .copied()
            .collect::<Vec<_>>();
        canvas.image.layer_stack().sort_by_visual_index(&mut layers);

        let head = layers.last().copied().unwrap();
        let head_parent = canvas.image.layer_stack().get_parent_of(&head).unwrap();
        let head_parent_node = canvas
            .image
            .layer_stack()
            .get_layer(head_parent.id())
            .unwrap();

        let (new_parent, new_position) =
            if let Some(sibling_id) = head_parent_node.child_above(&head) {
                // Parent node has a sibling. So the node won't go out of parent node.
                if canvas
                    .image
                    .layer_stack()
                    .can_have_children_of(&sibling_id, &head)
                    .unwrap()
                {
                    // Sibling node can have active layer as children, so move active layer into sibling node.
                    (sibling_id, LayerPosition::background())
                } else {
                    // If can't, swap them.
                    (*head_parent.id(), LayerPosition::above(sibling_id))
                }
            } else if let Some(head_parent_parent) = head_parent_node.parent().copied() {
                // Active node is the last child, so we are moving it out of its parent.
                (head_parent_parent, LayerPosition::above(*head_parent.id()))
            } else {
                return;
            };

        cx.push_undo_command_to_current(MoveLayersCommand::new(
            canvas,
            layers,
            new_parent,
            new_position,
        ))
        .log_err();
    }
}

impl ActionFunction for MoveLayerDownAction {
    fn trigger(&self, cx: &mut App) {
        let Some(canvas) = cx.read_current_canvas() else {
            return;
        };

        let mut layers = canvas
            .selected_layer_ids()
            .iter()
            .copied()
            .collect::<Vec<_>>();
        canvas.image.layer_stack().sort_by_visual_index(&mut layers);

        let tail = layers.first().copied().unwrap();
        let tail_parent = canvas.image.layer_stack().get_parent_of(&tail).unwrap();
        let tail_parent_node = canvas
            .image
            .layer_stack()
            .get_layer(tail_parent.id())
            .unwrap();

        let (new_parent, new_position) =
            if let Some(sibling_id) = tail_parent_node.child_below(&tail) {
                // Parent node has a sibling. So the node won't go out of parent node.
                if canvas
                    .image
                    .layer_stack()
                    .can_have_children_of(&sibling_id, &tail)
                    .expect("Sibling layer should always exist")
                {
                    // Sibling node can have active layer as children, so move active layer into sibling node.
                    (sibling_id, LayerPosition::foreground())
                } else {
                    // If can't, swap them.
                    (*tail_parent.id(), LayerPosition::below(sibling_id))
                }
            } else if let Some(tail_parent_parent) = tail_parent_node.parent().copied() {
                // Active node is the first child, so we are moving it out of its parent.
                (tail_parent_parent, LayerPosition::below(*tail_parent.id()))
            } else {
                return;
            };

        cx.push_undo_command_to_current(MoveLayersCommand::new(
            canvas,
            layers,
            new_parent,
            new_position,
        ))
        .log_err();
    }
}

impl ActionFunction for DeleteSelectedLayersAction {
    fn trigger(&self, cx: &mut App) {
        let Some(canvas) = cx.read_current_canvas() else {
            return;
        };

        if let Ok(cmd) = DeleteLayersCommand::new(
            canvas,
            canvas.selected_layer_ids().iter().copied().collect(),
        )
        .logged_err()
        {
            cx.push_undo_command_to_current(cmd).log_err();
        }
    }
}

impl ActionFunction for SelectPreviousLayerAction {
    fn trigger(&self, cx: &mut App) {
        cx.update_current_canvas(|canvas, cx| {
            let active_node = canvas.active_layer_node();
            let active_parent_node = canvas
                .image
                .layer_stack()
                .get_layer(active_node.parent().unwrap())
                .unwrap();
            if let Some(layer) = active_parent_node.child_above(active_node.id()) {
                // Find the closest *visual* sibling
                let mut current = canvas.image.layer_stack().get_layer(&layer).unwrap();
                while let Some(child) = current.children().first() {
                    current = canvas.image.layer_stack().get_layer(child).unwrap();
                }
                canvas.set_active_layer(*current.id(), cx);
            } else {
                canvas.set_active_layer(*active_parent_node.id(), cx);
            }
        });
        cx.refresh_windows();
    }
}

impl ActionFunction for SelectNextLayerAction {
    fn trigger(&self, cx: &mut App) {
        cx.update_current_canvas(|canvas, cx| {
            let active_node = canvas.active_layer_node();

            if let Some(child) = active_node.children().last() {
                canvas.set_active_layer(*child, cx);
                return;
            }

            let active_parent_node = canvas
                .image
                .layer_stack()
                .get_layer(active_node.parent().unwrap())
                .unwrap();

            if let Some(layer) = active_parent_node.child_below(active_node.id()) {
                // Not the last node
                canvas.set_active_layer(layer, cx);
                return;
            }

            // Is the last node, find the next *visual* sibling
            let mut current = active_parent_node;
            while let Some(current_parent) = current
                .parent()
                .and_then(|p| canvas.image.layer_stack().get_layer(p))
            {
                if let Some(layer) = current_parent.child_below(current.id()) {
                    canvas.set_active_layer(layer, cx);
                    return;
                }
                current = current_parent;
            }
        });
        cx.refresh_windows();
    }
}
