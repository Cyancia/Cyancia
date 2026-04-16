use std::sync::Arc;

use bevy_math::Rect;
use cyancia_assets::store::AssetRegistry;
use cyancia_image::tile::{GpuTileStorage, GpuTileStorageInner};
use cyancia_render::resources::{FullscreenVertex, GlobalSamplers};
use cyancia_runtime::Services;
use glam::{UVec2, Vec2};
use iced_core::{
    Clipboard, Element, Event, Layout, Length, Point, Rectangle, Shell, Size, Widget,
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

pub struct CanvasWidget<'a, Message> {
    pub is_focusing: bool,
    pub canvas: &'a CCanvas,
    pub tile_storage: GpuTileStorage,
    pub on_focus: Box<dyn Fn(Point) -> Message>,
    pub on_mouse_event: Box<dyn Fn(mouse::Event) -> Message>,
    pub on_widget_rect_change: Box<dyn Fn(Rect) -> Message>,
}

impl<'a, Message, Theme> Widget<Message, Theme, iced_wgpu::Renderer> for CanvasWidget<'a, Message> {
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
        let bounds = layout.bounds();
        let widget_rect = Rect {
            min: Vec2::new(bounds.x, bounds.y),
            max: Vec2::new(bounds.x + bounds.width, bounds.y + bounds.height),
        };
        if widget_rect != self.canvas.transform.widget_bounds {
            shell.publish((self.on_widget_rect_change)(widget_rect));
        }

        match event {
            Event::Mouse(event) => {
                if self.is_focusing {
                    shell.publish((self.on_mouse_event)(event.clone()));
                    shell.capture_event();
                } else if let mouse::Event::ButtonPressed(mouse::Button::Left) = event
                    && let Some(cursor_pos) = cursor.position_over(bounds)
                {
                    shell.publish((self.on_focus)(cursor_pos));
                    shell.publish((self.on_mouse_event)(event.clone()));
                    shell.capture_event();
                }
            }
            _ => {}
        }
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
                image_size: self.canvas.image.size(),
                root_layer: self.canvas.image.root().id(),
                transform: self.canvas.transform.clone(),
                tile_storage: self.tile_storage.clone(),
            },
        );
    }
}

impl<'a, Message, Theme> From<CanvasWidget<'a, Message>>
    for Element<'a, Message, Theme, iced_wgpu::Renderer>
where
    Message: 'a,
{
    fn from(canvas: CanvasWidget<'a, Message>) -> Self {
        Element::new(canvas)
    }
}
