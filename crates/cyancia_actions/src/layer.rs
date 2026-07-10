use std::{
    any::TypeId,
    io::{BufReader, Cursor},
};

use cyancia_canvas::{
    CCanvas, CanvasAppExt, CanvasUndoStackAppExt,
    command::{
        DeleteLayersCommand, GroupLayerCommand, InsertLayerCommand, LayerWithPosition,
        MoveLayersCommand,
    },
};
use cyancia_image::{
    CImage,
    blend_modes::BlendMode,
    composite::BlendFunction,
    layer::{LayerData, LayerId, LayerPosition, pixel_layer::PixelLayer},
    tile::TileStorageAppExt,
};
use cyancia_undo::BatchedUndoCommand;
use cyancia_utils::log_err::LogErr;
use gpui::{App, ClipboardEntry, actions};

use crate::ActionFunction;

actions!([
    CreateNewLayerAction,
    GroupSelectedLayersAction,
    MoveLayerUpAction,
    MoveLayerDownAction,
    DeleteSelectedLayersAction,
    SelectPreviousLayerAction,
    SelectNextLayerAction,
    PasteIntoNewLayerAction,
]);

fn find_proper_parent_position(canvas: &CCanvas) -> Option<(LayerId, LayerPosition)> {
    let mut cur_parent = canvas.active_layer_node();
    let mut cur_position = LayerPosition::foreground();
    while !cur_parent
        .data()
        .ty()
        .can_have_children_of(TypeId::of::<PixelLayer>())
    {
        let parent_id = canvas.image.layer_stack().get_layer(cur_parent.parent()?)?;
        cur_position = LayerPosition::above(*cur_parent.id());
        cur_parent = canvas
            .image
            .layer_stack()
            .get_layer(parent_id.id())
            .unwrap();
    }

    Some((*cur_parent.id(), cur_position))
}

impl ActionFunction for CreateNewLayerAction {
    fn trigger(&self, cx: &mut App) {
        let cmd = cx
            .update_current_canvas(|canvas, _| {
                let (parent, position) = find_proper_parent_position(canvas)?;

                let new_layer =
                    LayerData::new_normal_pixel(canvas.image.next_name_of_layer("Layer".into()));
                Some(InsertLayerCommand::new(canvas, new_layer, parent, position))
            })
            .unwrap()
            .unwrap();

        cx.push_undo_command_to_current(cmd).log_err();
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
                canvas.set_active_layer_and_clear_select(*current.id(), cx);
            } else {
                canvas.set_active_layer_and_clear_select(*active_parent_node.id(), cx);
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
                canvas.set_active_layer_and_clear_select(*child, cx);
                return;
            }

            let active_parent_node = canvas
                .image
                .layer_stack()
                .get_layer(active_node.parent().unwrap())
                .unwrap();

            if let Some(layer) = active_parent_node.child_below(active_node.id()) {
                // Not the last node
                canvas.set_active_layer_and_clear_select(layer, cx);
                return;
            }

            // Is the last node, find the next *visual* sibling
            let mut current = active_parent_node;
            while let Some(current_parent) = current
                .parent()
                .and_then(|p| canvas.image.layer_stack().get_layer(p))
            {
                if let Some(layer) = current_parent.child_below(current.id()) {
                    canvas.set_active_layer_and_clear_select(layer, cx);
                    return;
                }
                current = current_parent;
            }
        });
        cx.refresh_windows();
    }
}

impl ActionFunction for PasteIntoNewLayerAction {
    fn trigger(&self, cx: &mut App) {
        let Some(clipboard) = cx.read_from_clipboard() else {
            return;
        };

        let Some(entry) = clipboard.entries().iter().find(|e| {
            matches!(
                e,
                ClipboardEntry::ExternalPaths(_) | ClipboardEntry::Image(_)
            )
        }) else {
            return;
        };

        let Some(canvas) = cx.read_current_canvas() else {
            return;
        };

        let Some((parent, position)) = find_proper_parent_position(canvas) else {
            return;
        };

        match entry {
            ClipboardEntry::Image(image) => {
                let Ok((image, profile)) =
                    CImage::load_image_with_profile(BufReader::new(Cursor::new(image.bytes())))
                        .logged_err()
                else {
                    return;
                };

                let layer = LayerData::from_image(
                    "Pasted Image".into(),
                    image,
                    cx.tile_storage(),
                    BlendMode::Normal.id(),
                );

                let layer_storage = cx.tile_storage().get_layer(*layer.id()).unwrap();
                if layer_storage
                    .convert_color_space(&profile, canvas.image.profile(), Default::default())
                    .logged_err()
                    .is_err()
                {
                    return;
                }
                drop(layer_storage);

                let cmd = InsertLayerCommand::new(canvas, layer, parent, position);
                cx.push_undo_command_to_current(cmd).log_err();
            }
            ClipboardEntry::ExternalPaths(external_paths) => {
                let mut commands = Vec::new();
                let mut cur_position = position;

                for path in external_paths.paths() {
                    let Ok(layer) =
                        LayerData::from_path(path, cx.tile_storage(), canvas.image.profile())
                            .logged_err()
                    else {
                        continue;
                    };
                    let layer_id = *layer.id();
                    commands.push(InsertLayerCommand::new(canvas, layer, parent, cur_position));
                    cur_position = LayerPosition::above(layer_id);
                }

                cx.push_undo_command_to_current(BatchedUndoCommand::new(
                    "Paste Images".into(),
                    commands,
                ))
                .log_err();
            }
            _ => {}
        }
    }
}
