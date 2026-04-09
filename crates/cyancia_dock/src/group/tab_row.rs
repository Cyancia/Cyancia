use crate::{
    dock::{DockId, TabEvent},
    group::DockGroupData,
    style::DockCatalog,
};
use iced_core::{
    Element, Event, Font, Layout, Length, Point, Rectangle, Shell, Size, alignment,
    clipboard::Clipboard,
    layout, mouse, renderer,
    text::{self, LineHeight, Shaping},
    widget::{Tree, tree},
};

pub const TAB_WIDTH: f32 = 120.0;
pub const TAB_HEIGHT: f32 = 36.0;
const CLOSE_SIZE: f32 = 16.0;
const TAB_PAD: f32 = 8.0;
const DRAG_DEADBAND: f32 = 5.0;

// ── Tree state ────────────────────────────────────────────────────────────────

#[derive(Debug, Default, Clone, Copy)]
enum TabAction {
    #[default]
    Idle,
    Pressing {
        index: usize,
        origin: Point,
        is_close: bool,
    },
    Dragging {
        index: usize,
    },
}

#[derive(Debug, Default)]
struct TabRowState {
    action: TabAction,
    hovered: Option<usize>,
}

// ── Hit-testing ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy)]
enum TabHit {
    Tab(usize),
    Close(usize),
    Miss,
}

fn hit_test(group_data: &DockGroupData, bounds: Rectangle, cursor: Point) -> TabHit {
    if !bounds.contains(cursor) {
        return TabHit::Miss;
    }
    let rel_x = cursor.x - bounds.x;
    let n = group_data.len();
    if rel_x < 0.0 || n == 0 {
        return TabHit::Miss;
    }
    let tab_i = (rel_x / TAB_WIDTH) as usize;
    if tab_i >= n {
        return TabHit::Miss;
    }
    let close_x = bounds.x + (tab_i as f32 + 1.0) * TAB_WIDTH - TAB_PAD - CLOSE_SIZE;
    let close_y = bounds.y + (TAB_HEIGHT - CLOSE_SIZE) / 2.0;
    let close_rect = Rectangle {
        x: close_x,
        y: close_y,
        width: CLOSE_SIZE,
        height: CLOSE_SIZE,
    };
    if close_rect.contains(cursor) {
        TabHit::Close(tab_i)
    } else {
        TabHit::Tab(tab_i)
    }
}

const DETACH_DEADBAND_FACTOR: f32 = 0.5;

fn drag_target_index(
    group_data: &DockGroupData,
    bounds: Rectangle,
    cursor: Point,
) -> Option<usize> {
    let detach_min = bounds.y - bounds.height * DETACH_DEADBAND_FACTOR;
    let detach_max = bounds.y + bounds.height * (1.0 + DETACH_DEADBAND_FACTOR);
    if cursor.y < detach_min || cursor.y > detach_max {
        return None;
    }

    let rel_x = (cursor.x - bounds.x).max(0.0);
    let n = group_data.len();
    Some(((rel_x / TAB_WIDTH).round() as usize).min(n))
}

// ── Widget ────────────────────────────────────────────────────────────────────

pub struct TabRowWidget<'a, Message> {
    group_data: &'a DockGroupData,
    on_action: Box<dyn Fn(TabEvent) -> Message + 'a>,
    on_title_drag: Option<Box<dyn Fn() -> Message + 'a>>,
    title_of: Box<dyn Fn(&DockId) -> String + 'a>,
}

impl<'a, Message> TabRowWidget<'a, Message> {
    pub fn new(
        group_data: &'a DockGroupData,
        on_action: impl Fn(TabEvent) -> Message + 'a,
    ) -> Self {
        Self {
            group_data,
            on_action: Box::new(on_action),
            on_title_drag: None,
            title_of: Box::new(|id| id.to_string()),
        }
    }

    pub fn on_title_drag(mut self, f: impl Fn() -> Message + 'a) -> Self {
        self.on_title_drag = Some(Box::new(f));
        self
    }

    pub fn title_of(mut self, f: impl Fn(&DockId) -> String + 'a) -> Self {
        self.title_of = Box::new(f);
        self
    }
}

// ── Widget impl ───────────────────────────────────────────────────────────────

impl<'a, Message, Theme, Renderer> iced_core::Widget<Message, Theme, Renderer>
    for TabRowWidget<'a, Message>
where
    Message: 'a,
    Theme: DockCatalog,
    Renderer: iced_core::Renderer + iced_core::text::Renderer,
{
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<TabRowState>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(TabRowState::default())
    }

    fn children(&self) -> Vec<Tree> {
        vec![]
    }

    fn size(&self) -> Size<Length> {
        let width = if self.on_title_drag.is_some() {
            Length::Fill
        } else {
            Length::Shrink
        };
        Size::new(width, Length::Fixed(TAB_HEIGHT))
    }

    fn layout(
        &mut self,
        _tree: &mut Tree,
        _renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        let n = self.group_data.len().max(1) as f32;
        let intrinsic = Size::new(n * TAB_WIDTH, TAB_HEIGHT);
        if self.on_title_drag.is_some() {
            layout::Node::new(limits.resolve(Length::Fill, Length::Fixed(TAB_HEIGHT), intrinsic))
        } else {
            layout::Node::new(intrinsic)
        }
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut Renderer,
        theme: &Theme,
        _style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _viewport: &Rectangle,
    ) {
        let state = tree.state.downcast_ref::<TabRowState>();
        let bounds = layout.bounds();
        let dock_style = theme.style(
            &<Theme as DockCatalog>::default(),
            crate::style::DockStatus::Active,
        );
        let ts = &dock_style.tab_bar;

        let active = self.group_data.active();
        let drag_target = if let TabAction::Dragging { index: _ } = state.action {
            cursor
                .position()
                .and_then(|p| drag_target_index(self.group_data, bounds, p))
        } else {
            None
        };

        for (i, dock_id) in self.group_data.iter().enumerate() {
            let x = bounds.x + i as f32 * TAB_WIDTH;
            let tab_rect = Rectangle {
                x,
                y: bounds.y,
                width: TAB_WIDTH,
                height: TAB_HEIGHT,
            };

            let is_active = active == Some(dock_id);
            let is_hovered = state.hovered == Some(i);
            let tab_style = if is_active {
                &ts.active_tab
            } else if is_hovered {
                &ts.hovered_tab
            } else {
                &ts.inactive_tab
            };

            renderer.fill_quad(
                renderer::Quad {
                    bounds: tab_rect,
                    border: tab_style.border,
                    ..Default::default()
                },
                tab_style.background,
            );

            // Title text
            let title = (self.title_of)(dock_id);
            let text_x = x + TAB_PAD;
            let text_w = TAB_WIDTH - TAB_PAD * 2.0 - CLOSE_SIZE - TAB_PAD;
            renderer.fill_text(
                text::Text {
                    content: title,
                    size: iced_core::Pixels(14.0),
                    font: renderer.default_font(),
                    bounds: Size::new(text_w, TAB_HEIGHT),
                    align_x: text::Alignment::Left,
                    align_y: alignment::Vertical::Center,
                    line_height: LineHeight::default(),
                    shaping: Shaping::Basic,
                    wrapping: text::Wrapping::None,
                },
                Point::new(text_x, bounds.y + TAB_HEIGHT / 2.0),
                tab_style.text_color,
                tab_rect,
            );

            // Close button
            let close_x = x + TAB_WIDTH - TAB_PAD - CLOSE_SIZE;
            let close_y = bounds.y + (TAB_HEIGHT - CLOSE_SIZE) / 2.0;
            let close_rect = Rectangle {
                x: close_x,
                y: close_y,
                width: CLOSE_SIZE,
                height: CLOSE_SIZE,
            };
            let close_color = if state.hovered == Some(i) {
                ts.close_button_hover_color
            } else {
                ts.close_button_color
            };
            renderer.fill_text(
                text::Text {
                    content: "×".to_string(),
                    size: iced_core::Pixels(CLOSE_SIZE),
                    font: renderer.default_font(),
                    bounds: Size::new(CLOSE_SIZE, CLOSE_SIZE),
                    align_x: text::Alignment::Center,
                    align_y: alignment::Vertical::Center,
                    line_height: LineHeight::default(),
                    shaping: Shaping::Basic,
                    wrapping: text::Wrapping::None,
                },
                close_rect.center(),
                close_color,
                close_rect,
            );
        }

        // Drop indicator during tab drag
        if let Some(target) = drag_target {
            let x = bounds.x + target as f32 * TAB_WIDTH;
            let indicator = Rectangle {
                x: x - 1.5,
                y: bounds.y,
                width: 3.0,
                height: TAB_HEIGHT,
            };
            renderer.fill_quad(
                renderer::Quad {
                    bounds: indicator,
                    ..Default::default()
                },
                iced_core::Background::Color(iced_core::Color::from_rgb(0.3, 0.6, 1.0)),
            );
        }
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _renderer: &Renderer,
        _clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        _viewport: &Rectangle,
    ) {
        let bounds = layout.bounds();
        let state = tree.state.downcast_mut::<TabRowState>();

        match event {
            Event::Mouse(mouse::Event::CursorMoved { position }) => {
                // Update hover
                state.hovered = match hit_test(self.group_data, bounds, *position) {
                    TabHit::Tab(i) | TabHit::Close(i) => Some(i),
                    TabHit::Miss => None,
                };

                // Track drag
                if let TabAction::Pressing {
                    origin,
                    index,
                    is_close: false,
                } = state.action
                {
                    let dx = position.x - origin.x;
                    let dy = position.y - origin.y;
                    if (dx * dx + dy * dy).sqrt() >= DRAG_DEADBAND {
                        state.action = TabAction::Dragging { index };
                        shell.capture_event();
                    }
                } else if let TabAction::Dragging { index } = state.action {
                    shell.request_redraw();
                    shell.capture_event();

                    if drag_target_index(self.group_data, bounds, *position).is_none() {
                        let id = self.group_data.iter().nth(index).unwrap();
                        shell.publish((self.on_action)(TabEvent::Detach(id.clone())));
                        state.action = TabAction::Idle;
                    }
                }
            }

            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                if let Some(pos) = cursor.position() {
                    match hit_test(self.group_data, bounds, pos) {
                        TabHit::Tab(i) => {
                            state.action = TabAction::Pressing {
                                index: i,
                                origin: pos,
                                is_close: false,
                            };
                            shell.capture_event();
                        }
                        TabHit::Close(i) => {
                            state.action = TabAction::Pressing {
                                index: i,
                                origin: pos,
                                is_close: true,
                            };
                            shell.capture_event();
                        }
                        TabHit::Miss => {
                            // Non-tab title bar area: forward to on_title_drag if set.
                            // If not set, do not capture → pane_grid handles it as a drag pick.
                            if bounds.contains(pos) {
                                if let Some(f) = &self.on_title_drag {
                                    shell.publish(f());
                                    shell.capture_event();
                                }
                            }
                        }
                    }
                }
            }

            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                match state.action {
                    TabAction::Pressing {
                        index: i,
                        is_close: false,
                        ..
                    } => {
                        if let Some(dock_id) = self.group_data.iter().nth(i) {
                            shell.publish((self.on_action)(TabEvent::Select(dock_id.clone())));
                        }
                        state.action = TabAction::Idle;
                    }
                    TabAction::Pressing {
                        index: i,
                        is_close: true,
                        ..
                    } => {
                        if let Some(dock_id) = self.group_data.iter().nth(i) {
                            shell.publish((self.on_action)(TabEvent::Close(dock_id.clone())));
                        }
                        state.action = TabAction::Idle;
                    }
                    TabAction::Dragging { index: from } => {
                        if let Some(pos) = cursor.position()
                            && let Some(to) = drag_target_index(self.group_data, bounds, pos)
                        {
                            let to = if to > from { to - 1 } else { to };
                            if to != from {
                                shell.publish((self.on_action)(TabEvent::Reorder { from, to }));
                            }
                        }

                        state.action = TabAction::Idle;
                    }
                    TabAction::Idle => {}
                }

                shell.request_redraw();
            }

            _ => {}
        }
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _viewport: &Rectangle,
        _renderer: &Renderer,
    ) -> mouse::Interaction {
        let state = tree.state.downcast_ref::<TabRowState>();
        if let TabAction::Dragging { .. } = state.action {
            return mouse::Interaction::Grabbing;
        }
        if let Some(pos) = cursor.position() {
            match hit_test(self.group_data, layout.bounds(), pos) {
                TabHit::Tab(_) | TabHit::Close(_) => mouse::Interaction::Pointer,
                TabHit::Miss => {
                    if self.on_title_drag.is_some() && layout.bounds().contains(pos) {
                        mouse::Interaction::Grab
                    } else {
                        mouse::Interaction::default()
                    }
                }
            }
        } else {
            mouse::Interaction::default()
        }
    }
}

impl<'a, Message, Theme, Renderer> From<TabRowWidget<'a, Message>>
    for Element<'a, Message, Theme, Renderer>
where
    Message: 'a,
    Theme: DockCatalog + 'a,
    Renderer: iced_core::Renderer + iced_core::text::Renderer + 'a,
{
    fn from(w: TabRowWidget<'a, Message>) -> Self {
        Element::new(w)
    }
}
