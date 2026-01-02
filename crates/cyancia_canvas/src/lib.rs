use std::sync::Arc;

use cyancia_image::CImage;
use parking_lot::RwLock;

use crate::control::CanvasTransform;

pub mod control;
pub mod render;
pub mod resource;
pub mod widget;

#[derive(Debug)]
pub struct CCanvas {
    pub image: Arc<CImage>,
    pub transform: RwLock<CanvasTransform>,
}
