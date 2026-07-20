use std::{
    any::TypeId,
    io::{BufReader, Cursor},
};

use cyancia_canvas::{CanvasAppExt, CanvasUndoStackAppExt, command::InsertLayerCommand};
use cyancia_image::{
    CImage,
    layer::{LayerPosition, pixel_layer::PixelLayer, properties::NameProp},
    tile::TileStorageAppExt,
};
use cyancia_undo::{BatchedUndoCommand, UndoStacks};
use cyancia_utils::log_err::LogErr;
use gpui::{App, BorrowAppContext, ClipboardEntry, actions};

use crate::ActionFunction;

actions!([UndoAction, RedoAction, PasteIntoNewLayerAction]);

impl ActionFunction for UndoAction {
    fn trigger(&self, cx: &mut App) {
        let Some(cur_canvas) = cx.current_canvas_id() else {
            return;
        };
        cx.update_global::<UndoStacks, _>(|stacks, cx| {
            if let Some(stack) = stacks.get_mut(&*cur_canvas) {
                stack.undo(cx).log_err();
            }
        });
    }
}

impl ActionFunction for RedoAction {
    fn trigger(&self, cx: &mut App) {
        let Some(cur_canvas) = cx.current_canvas_id() else {
            return;
        };
        cx.update_global::<UndoStacks, _>(|stacks, cx| {
            if let Some(stack) = stacks.get_mut(&*cur_canvas) {
                stack.redo(cx).log_err();
            }
        });
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

        let (parent, position) = {
            let mut cur_parent = canvas.active_layer_node();
            let mut cur_position = LayerPosition::foreground();
            while !cur_parent
                .instance()
                .can_have_children_of(TypeId::of::<PixelLayer>())
            {
                let Some(cur_parent_id) = cur_parent.parent() else {
                    return;
                };
                let parent_id = canvas.image.layer_stack().get_layer(cur_parent_id).unwrap();
                cur_position = LayerPosition::above(*cur_parent.id());
                cur_parent = canvas
                    .image
                    .layer_stack()
                    .get_layer(parent_id.id())
                    .unwrap();
            }
            (*cur_parent.id(), cur_position)
        };

        match entry {
            ClipboardEntry::Image(image) => {
                let Ok((image, profile)) =
                    CImage::load_image_with_profile(BufReader::new(Cursor::new(image.bytes())))
                        .logged_err()
                else {
                    return;
                };

                let mut layer = PixelLayer::from_image(image, cx.tile_storage());
                layer.properties_mut().set(NameProp("Pasted Image".into()));

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
                        PixelLayer::from_path(path, cx.tile_storage(), canvas.image.profile())
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
