use std::sync::Arc;

use cyancia_assets::store::AssetRegistry;
use cyancia_image::tile::{GpuTileStorage, GpuTileStorageInner};
use cyancia_render::resources::{FullscreenVertex, GlobalSamplers};
use cyancia_runtime::Services;
use glam::{UVec2, Vec2};
use iced_core::{
    Clipboard, Element, Event, Layout, Length, Rectangle, Shell, Size, Widget,
    keyboard::{self, key},
    layout::{self, Limits},
    mouse, renderer,
    widget::{Tree, tree},
};
use iced_wgpu::primitive::Renderer;
use iced_widget::{renderer::wgpu::primitive, shader::Program};
use parking_lot::Mutex;

use crate::{
    CCanvas,
    render::{CanvasPrimitive, CanvasRenderer},
};

pub struct CanvasWidget {
    pub canvas: Arc<CCanvas>,
    pub renderer: Arc<Mutex<CanvasRenderer>>,
    pub tile_storage: GpuTileStorage,
}

impl<Message, Theme> Widget<Message, Theme, iced_wgpu::Renderer> for CanvasWidget {
    fn size(&self) -> Size<Length> {
        Size::new(Length::Fill, Length::Fill)
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &iced_wgpu::Renderer,
        limits: &Limits,
    ) -> layout::Node {
        layout::atomic(limits, Length::Fill, Length::Fill)
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &iced_wgpu::Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        viewport: &Rectangle,
    ) {
        self.canvas.transform.write().widget_size =
            Vec2::new(layout.bounds().width, layout.bounds().height);
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut iced_wgpu::Renderer,
        theme: &Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        renderer.draw_primitive(
            layout.bounds(),
            CanvasPrimitive {
                canvas: self.canvas.clone(),
                renderer: self.renderer.clone(),
                tile_storage: self.tile_storage.clone(),
            },
        );
    }
}

impl<'a, Message, Theme> From<CanvasWidget> for Element<'a, Message, Theme, iced_wgpu::Renderer> {
    fn from(canvas: CanvasWidget) -> Self {
        Element::new(canvas)
    }
}
