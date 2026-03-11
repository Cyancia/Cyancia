use bevy_math::Vec2;
use cyancia_math::curve::CubicCurve;
use iced_core::{
    Background, Color, Element, Length, Point, Rectangle, Size, Theme, Vector, Widget,
    keyboard::{self, key},
    layout,
    mouse::{self, Cursor},
    renderer::{self, Quad},
    widget::{self, tree},
};
use iced_graphics::geometry::{Frame, Path, Stroke};

pub struct CurveEdit<'a, Message, Theme>
where
    Theme: Catalog,
{
    pub curve: CubicCurve,
    pub width: Length,
    pub height: Length,
    pub resolution: usize,
    pub class: Theme::Class<'a>,
    pub on_point_created: Option<Box<dyn Fn(usize, Vec2) -> Message + 'a>>,
    pub on_point_moved: Option<Box<dyn Fn(usize, Vec2) -> Message + 'a>>,
    pub on_point_deleted: Option<Box<dyn Fn(usize) -> Message + 'a>>,
}

impl<'a, Message, Theme> CurveEdit<'a, Message, Theme>
where
    Theme: Catalog,
{
    pub fn new(curve: CubicCurve) -> Self {
        Self {
            curve,
            width: Length::Fill,
            height: Length::Fill,
            resolution: 100,
            class: Theme::default(),
            on_point_created: None,
            on_point_moved: None,
            on_point_deleted: None,
        }
    }

    pub fn width(mut self, width: impl Into<Length>) -> Self {
        self.width = width.into();
        self
    }

    pub fn height(mut self, height: impl Into<Length>) -> Self {
        self.height = height.into();
        self
    }

    pub fn resolution(mut self, resolution: usize) -> Self {
        self.resolution = resolution;
        self
    }

    pub fn on_point_created(mut self, callback: impl Fn(usize, Vec2) -> Message + 'a) -> Self {
        self.on_point_created = Some(Box::new(callback));
        self
    }

    pub fn on_point_moved(mut self, callback: impl Fn(usize, Vec2) -> Message + 'a) -> Self {
        self.on_point_moved = Some(Box::new(callback));
        self
    }

    pub fn on_point_deleted(mut self, callback: impl Fn(usize) -> Message + 'a) -> Self {
        self.on_point_deleted = Some(Box::new(callback));
        self
    }
}

impl<'a, Message, Theme, Renderer> Widget<Message, Theme, Renderer>
    for CurveEdit<'a, Message, Theme>
where
    Theme: Catalog,
    Renderer: iced_core::Renderer + iced_graphics::geometry::Renderer,
{
    fn size(&self) -> Size<Length> {
        Size::new(self.width, self.height)
    }

    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }

    fn layout(
        &mut self,
        tree: &mut widget::Tree,
        renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        layout::atomic(limits, self.width, self.height)
    }

    fn state(&self) -> tree::State {
        tree::State::new(State::default())
    }

    fn update(
        &mut self,
        tree: &mut widget::Tree,
        event: &iced_core::Event,
        layout: layout::Layout<'_>,
        cursor: Cursor,
        renderer: &Renderer,
        clipboard: &mut dyn iced_core::Clipboard,
        shell: &mut iced_core::Shell<'_, Message>,
        viewport: &Rectangle,
    ) {
        let state = tree.state.downcast_mut::<State>();
        let width = layout.bounds().width;
        let height = layout.bounds().height;
        let size = Vec2::new(width, height);
        let control_points = self.curve.control_points();

        match event {
            iced_core::Event::Mouse(mouse::Event::ButtonPressed(_)) => {
                let Some(cursor_pos) = cursor.position_in(layout.bounds()) else {
                    return;
                };
                let cursor_px = Vec2::new(cursor_pos.x, height - cursor_pos.y);
                let cursor_01 = cursor_px / size;

                let mut found_point = false;
                for (i, p) in control_points.iter().enumerate() {
                    let pixel = p * size;
                    if pixel.distance_squared(cursor_px) < 64.0 {
                        state.selected_point = Some(i);
                        found_point = true;
                        state.dragging = true;
                        dbg!();
                        break;
                    }

                    if pixel.x > cursor_px.x
                        && let Some(on_point_created) = self.on_point_created.as_ref()
                    {
                        shell.publish(on_point_created(i, cursor_01));
                        state.selected_point = Some(i);
                        state.dragging = true;
                        found_point = true;
                        dbg!();
                        break;
                    }
                }

                if !found_point && let Some(on_point_created) = self.on_point_created.as_ref() {
                    let i = control_points.len();
                    shell.publish(on_point_created(i, cursor_01));
                    state.selected_point = Some(i);
                    state.dragging = true;
                }

                dbg!();
                shell.capture_event();
            }
            iced_core::Event::Mouse(mouse::Event::CursorMoved { .. }) => {
                if !state.dragging {
                    return;
                }

                let Some(cursor_pos) = cursor.position() else {
                    return;
                };
                let cursor_px = Vec2::new(
                    cursor_pos.x - layout.bounds().x,
                    height - (cursor_pos.y - layout.bounds().y),
                );
                let cursor_01 = cursor_px / size;

                if let Some(selected) = state.selected_point {
                    let prev_x = if selected == 0 {
                        0.0
                    } else {
                        control_points[selected - 1].x
                    };
                    let next_x = if selected == control_points.len() - 1 {
                        1.0
                    } else {
                        control_points[selected + 1].x
                    };

                    let clamped =
                        Vec2::new(cursor_01.x.clamp(prev_x + 0.01, next_x - 0.01), cursor_01.y)
                            .clamp(Vec2::ZERO, Vec2::ONE);

                    if let Some(on_point_moved) = self.on_point_moved.as_ref() {
                        shell.publish(on_point_moved(selected, clamped));
                    }
                }

                shell.capture_event();
            }
            iced_core::Event::Mouse(mouse::Event::ButtonReleased(_)) => {
                if state.dragging {
                    state.dragging = false;
                    shell.capture_event();
                }
            }
            iced_core::Event::Keyboard(keyboard::Event::KeyPressed { key, .. }) => match key {
                keyboard::Key::Named(key::Named::Delete) => {
                    if !cursor.is_over(layout.bounds()) {
                        return;
                    }

                    if let Some(selected) = state.selected_point {
                        state.selected_point = None;
                        shell.capture_event();

                        if let Some(on_point_deleted) = self.on_point_deleted.as_ref() {
                            shell.publish(on_point_deleted(selected));
                        }
                    }
                }
                _ => {}
            },
            _ => {}
        }
    }

    fn draw(
        &self,
        tree: &widget::Tree,
        renderer: &mut Renderer,
        theme: &Theme,
        renderer_style: &renderer::Style,
        layout: layout::Layout<'_>,
        cursor: Cursor,
        viewport: &Rectangle,
    ) {
        let style = theme.style(&self.class);
        let bounds = layout.bounds();
        let width = bounds.width;
        let height = bounds.height;
        let size = Vec2::new(width, height);
        let state = tree.state.downcast_ref::<State>();

        let points = self
            .curve
            .subdivide(self.resolution)
            .into_iter()
            .map(|p| Vec2::new(p.x, 1.0 - p.y) * size)
            .collect::<Vec<_>>();
        let mut frame = Frame::new(renderer, bounds.size());

        renderer.fill_quad(
            Quad {
                bounds: layout.bounds(),
                ..Default::default()
            },
            style.background,
        );

        let path = Path::new(|b| {
            if let Some(first) = points.first() {
                b.move_to(Point { x: 0.0, y: first.y });
            }

            for p in &points {
                b.line_to(Point { x: p.x, y: p.y });
            }

            if let Some(last) = points.last() {
                b.line_to(Point {
                    x: width,
                    y: last.y,
                });
            }
        });
        frame.stroke(
            &path,
            Stroke {
                style: style.line_color.into(),
                width: 2.0,
                ..Default::default()
            },
        );

        for (i, p) in self.curve.control_points().iter().enumerate() {
            const SIZE: f32 = 10.0;
            let px = Vec2::new(p.x, 1.0 - p.y) * size;
            let p0 = px - SIZE * 0.5;

            let p = Point::new(p0.x, p0.y);
            let size = Size::new(SIZE, SIZE);
            if Some(i) == state.selected_point {
                frame.fill_rectangle(p, size, style.line_color);
            } else {
                frame.stroke_rectangle(
                    p,
                    size,
                    Stroke {
                        style: style.line_color.into(),
                        width: 2.0,
                        ..Default::default()
                    },
                );
            }
        }

        renderer.with_translation(Vector::new(bounds.x, bounds.y), |r| {
            r.draw_geometry(frame.into_geometry());
        });
    }
}

#[derive(Default)]
pub struct State {
    pub selected_point: Option<usize>,
    pub dragging: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Style {
    pub background: Background,
    pub line_color: Color,
}

pub trait Catalog: Sized {
    type Class<'a>;

    fn default<'a>() -> Self::Class<'a>;

    fn style(&self, class: &Self::Class<'_>) -> Style;
}

pub type StyleFn<'a, Theme> = Box<dyn Fn(&Theme) -> Style + 'a>;

impl Catalog for Theme {
    type Class<'a> = StyleFn<'a, Self>;

    fn default<'a>() -> Self::Class<'a> {
        Box::new(default)
    }

    fn style(&self, class: &Self::Class<'_>) -> Style {
        class(self)
    }
}

fn default(theme: &Theme) -> Style {
    Style {
        background: theme.extended_palette().background.base.color.into(),
        line_color: theme.extended_palette().primary.base.color.into(),
    }
}

impl<'a, Message, Theme, Renderer> Into<Element<'a, Message, Theme, Renderer>>
    for CurveEdit<'a, Message, Theme>
where
    Message: 'a,
    Theme: Catalog + 'a,
    Renderer: iced_core::Renderer + iced_graphics::geometry::Renderer + 'a,
{
    fn into(self) -> Element<'a, Message, Theme, Renderer> {
        Element::new(self)
    }
}
