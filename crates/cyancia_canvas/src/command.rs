use std::borrow::Cow;

use bevy_math::IRect;
use cyancia_image::{
    layer::{LayerData, LayerId},
    tile::{DynamicLayerStorage, GpuTileStorage, TileStorageAppExt},
};
use cyancia_render::render_context::RenderContextAppExt;
use cyancia_undo::UndoCommand;
use cyancia_utils::log_err::LogErr;
use glam::IVec2;
use gpui::App;
use wgpu::{
    Device, Extent3d, ImageSubresourceRange, Origin3d, Queue, TexelCopyTextureInfo, Texture,
    TextureAspect, TextureDescriptor, TextureDimension, TextureUsages,
};

use crate::{CanvasAppExt, CanvasId, event::CanvasUpdated};

pub struct TileReplaceCommand {
    pub reason: Cow<'static, str>,
    pub canvas: CanvasId,
    pub layer: LayerId,
    // The final update rect on undo/redo would be the union of these two.
    // When replacing tiles with one source, tiles only exist in another source will be cleared.
    pub old_tiles: Option<(Texture, Vec<IVec2>)>,
    pub new_tiles: Option<(Texture, Vec<IVec2>)>,
}

impl TileReplaceCommand {
    pub fn new(
        reason: Cow<'static, str>,
        canvas: CanvasId,
        device: &Device,
        queue: &Queue,
        layer_id: LayerId,
        layer_storage: &DynamicLayerStorage,
        // TODO accept Option
        new_tile_indices: Vec<IVec2>,
        new_tiles: Texture,
    ) -> Self {
        let old_tiles = layer_storage.texture().and_then(|layer_texture| {
            let old_tile_indices = new_tile_indices
                .iter()
                .copied()
                .filter(|i| layer_storage.get_tile(*i).is_some())
                .collect::<Vec<_>>();
            if old_tile_indices.is_empty() {
                return None;
            }
            let old_texture = device.create_texture(&TextureDescriptor {
                label: Some("old_texture"),
                size: Extent3d {
                    width: GpuTileStorage::TILE_SIZE,
                    height: GpuTileStorage::TILE_SIZE,
                    depth_or_array_layers: old_tile_indices.len() as u32,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: TextureDimension::D2,
                format: new_tiles.format(),
                usage: TextureUsages::COPY_DST | TextureUsages::COPY_SRC,
                view_formats: &[],
            });

            let mut ec = device.create_command_encoder(&Default::default());

            ec.push_debug_group("copy_old_tiles");
            for (dst_layer, index) in old_tile_indices.iter().enumerate() {
                let src_layer = layer_storage.get_tile_layer(*index).unwrap();
                ec.copy_texture_to_texture(
                    TexelCopyTextureInfo {
                        texture: layer_texture,
                        mip_level: 0,
                        origin: Origin3d {
                            x: 0,
                            y: 0,
                            z: src_layer,
                        },
                        aspect: TextureAspect::All,
                    },
                    TexelCopyTextureInfo {
                        texture: &old_texture,
                        mip_level: 0,
                        origin: Origin3d {
                            x: 0,
                            y: 0,
                            z: dst_layer as u32,
                        },
                        aspect: TextureAspect::All,
                    },
                    GpuTileStorage::TILE_COPY_SIZE,
                );
            }
            ec.pop_debug_group();

            queue.submit([ec.finish()]);

            Some((old_texture, old_tile_indices))
        });

        Self {
            reason,
            canvas,
            layer: layer_id,
            old_tiles,
            new_tiles: Some((new_tiles, new_tile_indices)),
        }
    }

    pub fn new_clear(
        reason: Cow<'static, str>,
        canvas: CanvasId,
        device: &Device,
        queue: &Queue,
        layer_id: LayerId,
        layer_storage: &DynamicLayerStorage,
    ) -> Self {
        let old_tiles = layer_storage.texture().map(|layer_texture| {
            let old_texture = device.create_texture(&TextureDescriptor {
                label: Some("old_texture"),
                size: layer_texture.size(),
                mip_level_count: 1,
                sample_count: 1,
                dimension: TextureDimension::D2,
                format: layer_texture.format(),
                usage: TextureUsages::COPY_DST | TextureUsages::COPY_SRC,
                view_formats: &[],
            });

            let mut ec = device.create_command_encoder(&Default::default());

            ec.push_debug_group("copy_old_tiles");
            ec.copy_texture_to_texture(
                layer_texture.as_image_copy(),
                old_texture.as_image_copy(),
                old_texture.size(),
            );
            ec.pop_debug_group();

            queue.submit([ec.finish()]);

            (
                old_texture,
                layer_storage.iter_tiles().map(|(i, _, _)| i).collect(),
            )
        });

        Self {
            reason,
            canvas,
            layer: layer_id,
            old_tiles,
            new_tiles: None,
        }
    }
}

fn apply_tile_replace(
    cx: &mut App,
    canvas: CanvasId,
    layer: LayerId,
    replace_tile: &Option<(Texture, Vec<IVec2>)>,
    clear_tile_indices: Vec<IVec2>,
) {
    let mut dirty_min = IVec2::MAX;
    let mut dirty_max = IVec2::MIN;

    let device = cx.render_device();
    let queue = cx.render_queue();

    let tile_storage = cx.tile_storage();
    let mut layer = tile_storage.get_layer_mut(layer).unwrap();

    let mut ec = device.create_command_encoder(&Default::default());

    if let Some((tiles, tile_indices)) = replace_tile {
        layer.allocate_tiles_batch(tile_indices);

        let layer_texture = layer.texture().unwrap();

        ec.push_debug_group("replace_old_with_new");
        for (src_layer, tile_index) in tile_indices.iter().enumerate() {
            let dst_layer = layer.get_tile_layer(*tile_index).unwrap();
            ec.copy_texture_to_texture(
                TexelCopyTextureInfo {
                    texture: tiles,
                    mip_level: 0,
                    origin: Origin3d {
                        x: 0,
                        y: 0,
                        z: src_layer as u32,
                    },
                    aspect: TextureAspect::All,
                },
                TexelCopyTextureInfo {
                    texture: layer_texture,
                    mip_level: 0,
                    origin: Origin3d {
                        x: 0,
                        y: 0,
                        z: dst_layer,
                    },
                    aspect: TextureAspect::All,
                },
                GpuTileStorage::TILE_COPY_SIZE,
            );

            dirty_min = dirty_min.min(*tile_index);
            dirty_max = dirty_max.max(*tile_index);
        }
        ec.pop_debug_group();
    }

    ec.push_debug_group("clear_old_without_new");
    for tile_index in clear_tile_indices {
        let Some(dst_layer) = layer.get_tile_layer(tile_index) else {
            continue;
        };

        ec.clear_texture(
            layer.texture_view().unwrap().texture(),
            &ImageSubresourceRange {
                aspect: TextureAspect::All,
                base_mip_level: 0,
                mip_level_count: None,
                base_array_layer: dst_layer,
                array_layer_count: Some(1),
            },
        );

        dirty_min = dirty_min.min(tile_index);
        dirty_max = dirty_max.max(tile_index);
    }
    ec.pop_debug_group();

    queue.submit([ec.finish()]);

    drop(layer);

    cx.update_canvas(&canvas, |_, cx| {
        cx.emit(CanvasUpdated {
            dirty_tiles: IRect {
                min: dirty_min,
                max: dirty_max + 1,
            },
        });
    });
}

impl UndoCommand for TileReplaceCommand {
    fn label(&self) -> Cow<'static, str> {
        self.reason.clone()
    }

    #[tracing::instrument(skip_all)]
    fn redo(&mut self, cx: &mut App) -> anyhow::Result<()> {
        let to_clear = self
            .old_tiles
            .as_ref()
            .map(|(_, i)| match &self.new_tiles {
                Some(new_tiles) => i
                    .iter()
                    .copied()
                    .filter(|old| !new_tiles.1.contains(old))
                    .collect::<Vec<_>>(),
                None => i.clone(),
            })
            .unwrap_or_default();

        apply_tile_replace(cx, self.canvas, self.layer, &self.new_tiles, to_clear);
        Ok(())
    }

    #[tracing::instrument(skip_all)]
    fn undo(&mut self, cx: &mut App) -> anyhow::Result<()> {
        let to_clear = self
            .new_tiles
            .as_ref()
            .map(|(_, i)| match &self.old_tiles {
                Some(old_tiles) => i
                    .iter()
                    .copied()
                    .filter(|new| !old_tiles.1.contains(new))
                    .collect::<Vec<_>>(),
                None => i.clone(),
            })
            .unwrap_or_default();

        apply_tile_replace(cx, self.canvas, self.layer, &self.old_tiles, to_clear);
        Ok(())
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

    fn can_cancel_out(&self, rhs: &dyn UndoCommand) -> bool {
        let Some(rhs) = rhs.downcast_ref::<Self>() else {
            return false;
        };

        self.canvas == rhs.canvas
            && self.layer == rhs.layer
            && self.new_parent == rhs.original_parent
            && self.new_index == rhs.original_index
            && self.original_parent == rhs.new_parent
            && self.original_index == rhs.new_index
    }
}
