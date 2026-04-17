use cyancia_widgets::drag_drop_column::DragDropColumn;
use iced_core::{
    Background, Color, Element, Layout, Length, Pixels, Point, Rectangle, Size, Widget,
    alignment::Vertical,
    layout, mouse, renderer,
    text::{Alignment, LineHeight, Shaping, Wrapping},
    widget,
};

use crate::layer::{Layer, LayerStack};

pub struct LayerNodeWidget<'a, Theme>
where
    Theme: Catalog,
{
    layer: &'a Layer,
    height: f32,
    font_size: Pixels,
    class: Theme::Class<'a>,
    depth: u32,
}

impl<'a, Theme> LayerNodeWidget<'a, Theme>
where
    Theme: Catalog,
{
    pub fn new(layer: &'a Layer) -> LayerNodeWidget<'a, Theme> {
        LayerNodeWidget {
            layer,
            height: 30.0,
            font_size: Pixels(16.0),
            class: <Theme as Catalog>::default(),
            depth: 0,
        }
    }

    pub fn height(mut self, height: f32) -> Self {
        self.height = height;
        self
    }

    pub fn class(mut self, class: Theme::Class<'a>) -> Self {
        self.class = class;
        self
    }

    pub fn font_size(mut self, font_size: Pixels) -> Self {
        self.font_size = font_size;
        self
    }

    pub fn depth(mut self, depth: u32) -> Self {
        self.depth = depth;
        self
    }
}

impl<Message, Theme, Renderer> Widget<Message, Theme, Renderer> for LayerNodeWidget<'_, Theme>
where
    Theme: Catalog,
    Renderer: iced_core::Renderer + iced_core::text::Renderer,
{
    fn size(&self) -> Size<Length> {
        Size::new(Length::Fill, Length::Fixed(self.height))
    }

    fn layout(
        &mut self,
        tree: &mut widget::Tree,
        renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        layout::atomic(limits, Length::Fill, Length::Fixed(self.height))
    }

    fn draw(
        &self,
        tree: &widget::Tree,
        renderer: &mut Renderer,
        theme: &Theme,
        _style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        let style = theme.style(&self.class, Status::Idle);

        renderer.fill_quad(
            renderer::Quad {
                bounds: layout.bounds(),
                ..renderer::Quad::default()
            },
            style.background,
        );
        let indent = self.depth as f32 * 20.0;
        let bounds = Rectangle {
            x: layout.bounds().x + indent,
            y: layout.bounds().y,
            width: layout.bounds().width - indent,
            height: layout.bounds().height,
        };

        renderer.fill_text(
            iced_core::Text {
                content: self.layer.name().to_string(),
                bounds: bounds.size(),
                size: self.font_size,
                line_height: LineHeight::Relative(1.0),
                font: renderer.default_font(),
                align_x: Alignment::Left,
                align_y: Vertical::Center,
                shaping: Shaping::Auto,
                wrapping: Wrapping::None,
            },
            Point::new(bounds.x + 10.0, bounds.center_y()),
            style.text_color,
            bounds,
        );
    }
}

impl<'a, Message, Theme, Renderer> From<LayerNodeWidget<'a, Theme>>
    for Element<'a, Message, Theme, Renderer>
where
    Message: 'a,
    Theme: Catalog + 'a,
    Renderer: iced_core::Renderer + iced_core::text::Renderer,
{
    fn from(widget: LayerNodeWidget<'a, Theme>) -> Self {
        Element::new(widget)
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Style {
    pub background: Background,
    pub text_color: Color,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Idle,
    Selected,
}

pub trait Catalog {
    type Class<'a>;

    fn default<'a>() -> Self::Class<'a>;

    fn style(&self, class: &Self::Class<'_>, status: Status) -> Style;
}

pub type StyleFn<'a, Theme> = Box<dyn Fn(&Theme, Status) -> Style + 'a>;

impl Catalog for iced_core::Theme {
    type Class<'a> = StyleFn<'a, iced_core::Theme>;

    fn default<'a>() -> Self::Class<'a> {
        Box::new(primary)
    }

    fn style(&self, class: &Self::Class<'_>, status: Status) -> Style {
        class(self, status)
    }
}

fn primary(theme: &iced_core::Theme, status: Status) -> Style {
    let palette = theme.extended_palette();
    match status {
        Status::Idle => Style {
            background: Background::Color(palette.background.base.color),
            text_color: palette.background.base.text,
        },
        Status::Selected => Style {
            background: Background::Color(palette.primary.base.color),
            text_color: palette.primary.base.text,
        },
    }
}
