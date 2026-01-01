use std::sync::Arc;

use cyancia_canvas::CCanvas;
use cyancia_id::Id;
use cyancia_image::{CImage, layer::Layer, tile::GPU_TILE_STORAGE};
use cyancia_input::{action::Action, key::KeySequence};
use glam::UVec2;
use rfd::FileDialog;

use crate::{ActionFunction, shell::CShell};

#[derive(Default)]
pub struct OpenFileAction {}

impl ActionFunction for OpenFileAction {
    fn id(&self) -> Id<Action> {
        Id::from_str("open_file_action")
    }

    fn trigger(&self, shell: &mut CShell) {
        let Some(file) = FileDialog::new().pick_file() else {
            log::error!("Unable to get selected file path.");
            return;
        };

        let img = match image::open(&file) {
            Ok(i) => i,
            Err(e) => {
                log::error!("Unable to open image from file {:?}: {}", file, e);
                return;
            }
        };
        log::info!("Opened image from file {:?}.", file);

        let width = img.width();
        let height = img.height();
        let layer = Layer::from_image(img, &GPU_TILE_STORAGE);
        let canvas = CCanvas {
            image: Arc::new(CImage::from_layer(UVec2::new(width, height), layer)),
        };
        shell.request_canvas_creation(Arc::new(canvas));
    }
}
