use bevy_math::Rect;
use glam::Vec2;
use iced_core::{
    Clipboard, Element, Event, Layout, Length, Point, Rectangle, Shell, Size, Widget,
    layout::{self, Limits},
    mouse, renderer, touch,
    widget::Tree,
};
use iced_wgpu::primitive::Renderer;
use lapiz_image::{texel::TexelType, tile::GpuTileStorage};
use moxcms::ColorProfile;

use crate::{CCanvas, render::CanvasPrimitive};

pub struct CanvasWidget<'a, Message> {
    pub is_focusing: bool,
    pub canvas: &'a CCanvas,
    pub tile_storage: GpuTileStorage,
    pub on_focus: Box<dyn Fn(Point) -> Message + 'a>,
    pub on_mouse_event: Box<dyn Fn(mouse::Event) -> Message + 'a>,
    pub on_widget_rect_change: Box<dyn Fn(Rect) -> Message + 'a>,
    pub color_profile: ColorProfile,
    pub window_id: u64,
    pub monitor_name: String,
}

impl<Message, Theme> Widget<Message, Theme, iced_wgpu::Renderer> for CanvasWidget<'_, Message> {
    fn size(&self) -> Size<Length> {
        Size::new(Length::Fill, Length::Fill)
    }

    fn layout(&mut self, _: &mut Tree, _: &iced_wgpu::Renderer, limits: &Limits) -> layout::Node {
        layout::atomic(limits, Length::Fill, Length::Fill)
    }

    fn update(
        &mut self,
        _: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _: &iced_wgpu::Renderer,
        _: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        _: &Rectangle,
    ) {
        let bounds = layout.bounds();
        let widget_rect = Rect {
            min: Vec2::new(bounds.x, bounds.y),
            max: Vec2::new(bounds.x + bounds.width, bounds.y + bounds.height),
        };
        if widget_rect != self.canvas.transform.widget_bounds {
            shell.publish((self.on_widget_rect_change)(widget_rect));
        }

        if let Event::Mouse(event) = event
            && let Some(cursor_pos) = cursor.land().position_over(bounds)
        {
            if self.is_focusing {
                shell.publish((self.on_mouse_event)(*event));
                shell.capture_event();
            } else if let mouse::Event::ButtonPressed(mouse::Button::Left) = event {
                shell.publish((self.on_focus)(cursor_pos));
                shell.publish((self.on_mouse_event)(*event));
                shell.capture_event();
            }
        }

        if let Event::Touch(event) = event {
            match event {
                touch::Event::FingerPressed { position, .. } => {
                    shell.publish((self.on_focus)(*position));
                    shell.publish((self.on_mouse_event)(mouse::Event::ButtonPressed(
                        mouse::Button::Left,
                    )));
                    shell.capture_event();
                }
                touch::Event::FingerMoved { position, .. } if self.is_focusing => {
                    shell.publish((self.on_mouse_event)(mouse::Event::CursorMoved {
                        position: *position,
                    }));
                    shell.capture_event();
                }
                touch::Event::FingerLifted { .. } => {
                    shell.publish((self.on_mouse_event)(mouse::Event::ButtonReleased(
                        mouse::Button::Left,
                    )));
                    shell.capture_event();
                }
                _ => {}
            }
        }
    }

    fn draw(
        &self,
        _: &Tree,
        renderer: &mut iced_wgpu::Renderer,
        _: &Theme,
        _: &renderer::Style,
        layout: Layout<'_>,
        _: mouse::Cursor,
        _: &Rectangle,
    ) {
        renderer.draw_primitive(
            layout.bounds(),
            CanvasPrimitive {
                image_size: self.canvas.image.size(),
                root_layer: *self.canvas.image.layer_stack().root_id(),
                selection_layer: self.canvas.image.selection_layer(),
                root_texel_type: self.canvas.image.texel_type(),
                selection_texel_type: TexelType::A8,
                transform: self.canvas.transform.clone(),
                tile_storage: self.tile_storage.clone(),
                color_profile: self.color_profile.clone(),
                window_id: self.window_id,
                monitor_name: self.monitor_name.clone(),
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
