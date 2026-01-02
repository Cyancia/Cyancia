use iced_wgpu::core::{
    Layout, Length, Rectangle, Size, Widget,
    layout::{self, Limits},
    mouse, renderer,
    widget::Tree,
};
use wgpu::{Device, Queue};

/// HACK:
/// We really really really need the device and queue to initialize resources, as well as
/// performing background tasks.
/// Iced currently don't expose the renderer except inside widgets.
/// So in custom fork of iced_wgpu, we added `device()` and `queue()` methods to the renderer,
/// and we can get the device and queue now.
pub struct RendererAcquire<Message> {
    pub on_acquire: Box<dyn Fn(Device, Queue) -> Message + Send + Sync>,
}

impl<Message, Theme> Widget<Message, Theme, iced_wgpu::Renderer> for RendererAcquire<Message> {
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
        event: &iced_wgpu::core::Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &iced_wgpu::Renderer,
        clipboard: &mut dyn iced_wgpu::core::Clipboard,
        shell: &mut iced_wgpu::core::Shell<'_, Message>,
        viewport: &Rectangle,
    ) {
        shell.publish((self.on_acquire)(
            renderer.device().clone(),
            renderer.queue().clone(),
        ));
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
    }
}
