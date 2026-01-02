use std::sync::Arc;

use cyancia_canvas::CCanvas;
use cyancia_id::Id;
use cyancia_image::{CImage, layer::Layer, tile::GPU_TILE_STORAGE};
use cyancia_input::{action::Action, key::KeySequence};
use glam::UVec2;
use iced_runtime::Task;
use rfd::{AsyncFileDialog, FileDialog};

use crate::{ActionFunction, shell::ActionShell, task::ActionTask};

#[derive(Default)]
pub struct OpenFileAction {}

impl ActionFunction for OpenFileAction {
    fn id(&self) -> Id<Action> {
        Id::from_str("open_file_action")
    }

    fn trigger(&self, shell: &mut ActionShell) {
        shell.queue_task(Task::future(load_image()));
    }
}

pub struct OpenFileTask {
    canvas: CCanvas,
}

impl ActionTask for OpenFileTask {
    fn apply(self: Box<Self>, shell: &mut ActionShell) {
        shell.set_current_canvas(Arc::new(self.canvas));
    }
}

async fn load_image() -> Option<OpenFileTask> {
    let Some(file) = AsyncFileDialog::new().pick_file().await else {
        log::error!("Unable to get selected file path.");
        return None;
    };

    let img = match image::load_from_memory(&file.read().await) {
        Ok(i) => i,
        Err(e) => {
            log::error!("Unable to open image from file {:?}: {}", file, e);
            return None;
        }
    };
    log::info!("Opened image from file {:?}.", file);

    let width = img.width();
    let height = img.height();
    let layer = Layer::from_image(img, &GPU_TILE_STORAGE);
    let canvas = CCanvas {
        image: Arc::new(CImage::from_layer(UVec2::new(width, height), layer)),
        transform: Default::default(),
    };

    Some(OpenFileTask { canvas })
}
