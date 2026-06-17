use std::borrow::Cow;

use bevy_math::IRect;
use cyancia_image::{
    dynamic_intermediate_buffer::DynamicGpuTileInfoBuffer,
    layer::{LayerData, LayerId},
    tile::{DynamicLayerStorage, GpuTileStorage, GpuTileStorageInner},
};
use cyancia_render::render_context::RenderContext;
use cyancia_undo::UndoCommand;
use cyancia_utils::log_err::LogErr;
use glam::IVec2;
use gpui::App;
use log::info;
use wgpu::{
    Device, Extent3d, ImageSubresourceRange, Origin3d, Queue, TexelCopyTextureInfo, Texture,
    TextureAspect, TextureDescriptor, TextureDimension, TextureUsages,
};

use crate::{CanvasAppExt, CanvasId, event::CanvasUpdated};

pub struct TileReplaceCommand {
    pub reason: Cow<'static, str>,
    pub canvas: CanvasId,
    pub layer: LayerId,
    // If this is empty, means the tiles does not exist before replacement.
    pub old_tiles: Option<(Texture, Vec<IVec2>)>,
    // If this is empty, means the tiles are cleared after replacement.
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
        new_tile_indices: Vec<IVec2>,
        new_tiles: Texture,
    ) -> Self {
        let old_tiles = layer_storage.texture().map(|layer_texture| {
            let old_tile_indices = new_tile_indices
                .iter()
                .copied()
                .filter(|i| layer_storage.get_tile(*i).is_some())
                .collect::<Vec<_>>();
            let old_texture = device.create_texture(&TextureDescriptor {
                label: Some("old_texture"),
                size: Extent3d {
                    width: GpuTileStorageInner::TILE_SIZE,
                    height: GpuTileStorageInner::TILE_SIZE,
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

            for (dst_layer, index) in old_tile_indices.iter().enumerate() {
                let src_layer = layer_storage.get_tile_layer(*index).unwrap();
                ec.copy_texture_to_texture(
                    TexelCopyTextureInfo {
                        texture: layer_texture.texture(),
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
                    GpuTileStorageInner::TILE_COPY_SIZE,
                );
            }

            queue.submit([ec.finish()]);

            (old_texture, old_tile_indices)
        });

        Self {
            reason,
            canvas,
            layer: layer_id,
            old_tiles,
            new_tiles: Some((new_tiles, new_tile_indices)),
        }
    }
}

fn apply_tile_replace(
    cx: &mut App,
    canvas: CanvasId,
    layer: LayerId,
    tile_indices: &Vec<IVec2>,
    tiles: &Texture,
) {
    let render_context = cx.global::<RenderContext>().clone();
    let device = render_context.device.clone();
    let queue = render_context.queue.clone();

    let tile_storage = cx.global_mut::<GpuTileStorage>();
    let mut layer = tile_storage.get_layer_mut(layer).unwrap();

    for index in tile_indices {
        layer.get_tile_or_allocate(*index);
    }

    let layer_texture = layer.texture().unwrap().texture();

    let mut ec = device.create_command_encoder(&Default::default());
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
                texture: &layer_texture,
                mip_level: 0,
                origin: Origin3d {
                    x: 0,
                    y: 0,
                    z: dst_layer,
                },
                aspect: TextureAspect::All,
            },
            GpuTileStorageInner::TILE_COPY_SIZE,
        );
    }
    queue.submit([ec.finish()]);

    drop(layer);

    cx.update_canvas(&canvas, |_, cx| {
        let mut min = IVec2::MAX;
        let mut max = IVec2::ZERO;
        for tile_index in tile_indices {
            min = min.min(*tile_index);
            max = max.max(*tile_index);
        }

        cx.emit(CanvasUpdated {
            dirty_tiles: IRect { min, max: max + 1 },
        });
    });
}

fn clear_tiles(cx: &mut App, canvas: CanvasId, layer: LayerId, tile_indices: &Vec<IVec2>) {
    let render_context = cx.global::<RenderContext>();
    let device = render_context.device.clone();
    let queue = render_context.queue.clone();

    let tile_storage = cx.global_mut::<GpuTileStorage>();
    let layer = tile_storage.get_layer_mut(layer).unwrap();

    let Some(layer_texture) = layer.texture().map(|v| v.texture()) else {
        return;
    };

    let mut ec = device.create_command_encoder(&Default::default());
    for tile_index in tile_indices {
        let dst_layer = layer.get_tile_layer(*tile_index).unwrap();
        ec.clear_texture(
            layer_texture,
            &ImageSubresourceRange {
                aspect: TextureAspect::All,
                base_mip_level: 0,
                mip_level_count: None,
                base_array_layer: dst_layer as u32,
                array_layer_count: Some(1),
            },
        );
    }
    queue.submit([ec.finish()]);

    drop(layer);

    cx.update_canvas(&canvas, |_, cx| {
        let mut min = IVec2::MAX;
        let mut max = IVec2::ZERO;
        for tile_index in tile_indices {
            min = min.min(*tile_index);
            max = max.max(*tile_index);
        }
        cx.emit(CanvasUpdated {
            dirty_tiles: IRect { min, max: max + 1 },
        });
    });
}

impl UndoCommand for TileReplaceCommand {
    fn label(&self) -> Cow<'static, str> {
        self.reason.clone()
    }

    fn redo(&mut self, cx: &mut App) -> anyhow::Result<()> {
        if let Some((texture, tiles)) = &self.new_tiles {
            apply_tile_replace(cx, self.canvas, self.layer, tiles, texture);
            info!(
                "{}: Replaced {} tiles: {:?}",
                self.reason,
                tiles.len(),
                tiles
            );
        } else if let Some((_, tiles)) = &self.old_tiles {
            clear_tiles(cx, self.canvas, self.layer, tiles);
            info!(
                "{}: Cleared {} tiles: {:?}",
                self.reason,
                tiles.len(),
                tiles
            );
        }
        Ok(())
    }

    fn undo(&mut self, cx: &mut App) -> anyhow::Result<()> {
        if let Some((texture, tiles)) = &self.old_tiles {
            apply_tile_replace(cx, self.canvas, self.layer, tiles, texture);
            info!(
                "{}: Undo replaced {} tiles: {:?}",
                self.reason,
                tiles.len(),
                tiles
            );
        } else if let Some((_, tiles)) = &self.new_tiles {
            clear_tiles(cx, self.canvas, self.layer, tiles);
            info!(
                "{}: Undo cleared {} tiles: {:?}",
                self.reason,
                tiles.len(),
                tiles
            );
        }
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
}
