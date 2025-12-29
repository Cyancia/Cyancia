use iced_core::{
    Color, Element, Layout, Length, Rectangle, Size, Widget, border,
    layout::{self, Limits},
    mouse,
    renderer::{self, Quad},
    widget::Tree,
};

pub struct Circle {
    pub color: Color,
    pub radius: f32,
}

impl<Message, Theme, Renderer> Widget<Message, Theme, Renderer> for Circle
where
    Renderer: iced_core::renderer::Renderer,
{
    fn size(&self) -> Size<Length> {
        let d = Length::Fixed(self.radius * 2.0);
        Size::new(d, d)
    }

    fn layout(&mut self, tree: &mut Tree, renderer: &Renderer, limits: &Limits) -> layout::Node {
        let d = self.radius * 2.0;
        layout::Node::new(Size::new(d, d))
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut Renderer,
        theme: &Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        renderer.fill_quad(
            Quad {
                bounds: layout.bounds(),
                border: border::rounded(self.radius),
                ..Default::default()
            },
            self.color,
        );
    }
}

impl<'a, Message, Theme, Renderer> Into<Element<'a, Message, Theme, Renderer>> for Circle
where
    Renderer: iced_core::renderer::Renderer,
{
    fn into(self) -> Element<'a, Message, Theme, Renderer> {
        Element::new(self)
    }
}
