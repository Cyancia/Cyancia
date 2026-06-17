use std::borrow::Cow;

use anyhow::bail;
use cyancia_canvas::{CCanvas, CanvasAppExt, CanvasId, CanvasUndoStackAppExt};
use cyancia_image::{
    layer::{LayerData, LayerId, LayerStackNode},
    texel::TexelType,
    tile::{GpuLayerInfo, GpuTileStorage},
};
use cyancia_undo::UndoCommand;
use cyancia_utils::log_err::LogErr;
use gpui::{App, WeakEntity, actions};

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
                    index,
                    previous_active_layer: active_layer_id,
                }
            })
            .unwrap();

        let new_layer_id = cmd.layer.id();

        if cx.push_undo_command_to_current(cmd).logged_err().is_err() {
            return;
        }

        let tiles = cx.global::<GpuTileStorage>();
        tiles.declare_layer(
            new_layer_id,
            GpuLayerInfo {
                // TODO use image format
                texel_type: TexelType::RGBA8,
            },
        );
    }
}

pub struct InsertLayerCommand {
    pub canvas: CanvasId,
    pub layer: LayerData,
    pub parent_id: LayerId,
    pub index: usize,
    pub previous_active_layer: LayerId,
}

impl UndoCommand for InsertLayerCommand {
    fn label(&self) -> Cow<'static, str> {
        "Create Layer".into()
    }

    fn redo(&mut self, cx: &mut App) -> anyhow::Result<()> {
        cx.update_canvas(&self.canvas, |canvas, _| {
            canvas.image.layer_stack_mut().add_layer(
                self.parent_id,
                self.index,
                self.layer.clone(),
            );
            canvas.image.active_layer = self.layer.id();
        })
        .ok_or(anyhow::anyhow!("Canvas {} not found", self.canvas))
        .log_err();
        cx.refresh_windows();

        Ok(())
    }

    fn undo(&mut self, cx: &mut App) -> anyhow::Result<()> {
        cx.update_canvas(&self.canvas, |canvas, _| {
            canvas.image.layer_stack_mut().remove_layer(self.layer.id());
            canvas.image.active_layer = self.previous_active_layer;
        })
        .ok_or(anyhow::anyhow!("Canvas {} not found", self.canvas))
        .log_err();
        cx.refresh_windows();

        Ok(())
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

pub struct GroupLayerCommand {
    pub canvas: CanvasId,
    pub group: LayerData,
    pub children: Vec<GroupedLayer>,
    pub parent_id: LayerId,
    pub index: usize,
    pub previous_active_layer: LayerId,
}

pub struct GroupedLayer {
    pub id: LayerId,
    pub original_parent: LayerId,
    pub original_index: usize,
}

impl UndoCommand for GroupLayerCommand {
    fn label(&self) -> Cow<'static, str> {
        "Group Layer".into()
    }

    fn redo(&mut self, cx: &mut App) -> anyhow::Result<()> {
        cx.update_canvas(&self.canvas, |canvas, _| {
            canvas.image.layer_stack_mut().add_layer(
                self.parent_id,
                self.index,
                self.group.clone(),
            );
            for (i, child) in self.children.iter().enumerate() {
                canvas
                    .image
                    .layer_stack_mut()
                    .move_layer(child.id, self.group.id(), i);
            }
        })
        .ok_or(anyhow::anyhow!("Canvas {} not found", self.canvas))
        .log_err();
        cx.refresh_windows();

        Ok(())
    }

    fn undo(&mut self, cx: &mut App) -> anyhow::Result<()> {
        cx.update_canvas(&self.canvas, |canvas, _| {
            let children = self
                .children
                .iter()
                .map(|ch| canvas.image.layer_stack_mut().remove_layer(ch.id).unwrap())
                .collect::<Vec<_>>();
            // This must be done before moving children, because on of the children has
            // same parent with the group layer, AND it's before the group layer index,
            // then the original index of the child will be incorrect.
            canvas
                .image
                .layer_stack_mut()
                .remove_layer(self.group.id())
                .unwrap();
            // TODO: Here's actually a pitfall. We have to ensure the children are stored in correct order.
            //       If child A at index 0 is before child B at index 1, they should be stored in the order
            //       child A and child B, then this insertion works.
            //       Otherwise B will be inserted before A, which is incorrect.
            //       Sort it first probably.
            for (child, (data, node)) in self.children.iter().zip(children) {
                let original_parent = canvas
                    .image
                    .layer_stack_mut()
                    .find_node_mut(child.original_parent)
                    .unwrap();
                original_parent.insert_child(child.original_index, node);
                canvas.image.layer_stack_mut().insert_isolated_layer(data);
            }
        })
        .ok_or(anyhow::anyhow!("Canvas {} not found", self.canvas))
        .log_err();
        cx.refresh_windows();

        Ok(())
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

pub struct MoveLayerCommand {
    pub canvas: CanvasId,
    pub layer: LayerId,
    pub original_parent: LayerId,
    pub original_index: usize,
    pub new_parent: LayerId,
    pub new_index: usize,
}

impl UndoCommand for MoveLayerCommand {
    fn label(&self) -> Cow<'static, str> {
        "Move Layer".into()
    }

    fn redo(&mut self, cx: &mut App) -> anyhow::Result<()> {
        cx.update_canvas(&self.canvas, |canvas, _| {
            canvas
                .image
                .layer_stack_mut()
                .move_layer(self.layer, self.new_parent, self.new_index);
        });
        cx.refresh_windows();

        Ok(())
    }

    fn undo(&mut self, cx: &mut App) -> anyhow::Result<()> {
        cx.update_canvas(&self.canvas, |canvas, _| {
            canvas.image.layer_stack_mut().move_layer(
                self.layer,
                self.original_parent,
                self.original_index,
            );
        });
        cx.refresh_windows();

        Ok(())
    }
}
