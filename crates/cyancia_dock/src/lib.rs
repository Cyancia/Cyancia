pub mod dock;
pub mod group;
pub mod style;
pub mod state;

pub use dock::{DockAction, DockId, FloatAction, TabEvent};
pub use group::{DockGroupData, TabRowWidget};
pub use state::DockState;
pub use style::{DockCatalog, DockStyle, DockStatus, TabBarStyle, TabStyle};

use iced_widget::pane_grid;

// ── DockWidget ────────────────────────────────────────────────────────────────

/// Top-level widget that renders the docking system backed by `PaneGrid`.
pub struct DockWidget<'a, Message> {
    state: &'a DockState,
    content: Box<dyn Fn(pane_grid::Pane, DockId) -> iced::Element<'a, Message> + 'a>,
    on_action: Box<dyn Fn(DockAction) -> Message + 'a>,
    spacing: f32,
    drag_hint: Option<iced::Point>,
}

impl<'a, Message: Clone + 'a> DockWidget<'a, Message> {
    pub fn new(
        state: &'a DockState,
        on_action: impl Fn(DockAction) -> Message + 'a,
    ) -> Self {
        Self {
            state,
            content: Box::new(|_, id| iced::widget::text(id.to_string()).into()),
            on_action: Box::new(on_action),
            spacing: 2.0,
            drag_hint: None,
        }
    }

    pub fn content(
        mut self,
        f: impl Fn(pane_grid::Pane, DockId) -> iced::Element<'a, Message> + 'a,
    ) -> Self {
        self.content = Box::new(f);
        self
    }

    pub fn spacing(mut self, s: f32) -> Self {
        self.spacing = s;
        self
    }

    /// Show a drop-target highlight at the given position (pane_grid-relative
    /// logical coordinates).  Used to preview re-attach from a floating window.
    pub fn drag_hint(mut self, pos: iced::Point) -> Self {
        self.drag_hint = Some(pos);
        self
    }
}

impl<'a, Message: Clone + 'a> From<DockWidget<'a, Message>> for iced::Element<'a, Message> {
    fn from(w: DockWidget<'a, Message>) -> Self {
        use std::rc::Rc;

        let DockWidget { state, content, on_action, spacing, drag_hint } = w;

        let on_action: Rc<dyn Fn(DockAction) -> Message + 'a> = Rc::from(on_action);
        let a_content = Rc::clone(&on_action);
        let a_click   = Rc::clone(&on_action);
        let a_drag    = Rc::clone(&on_action);
        let a_resize  = Rc::clone(&on_action);

        let grid: iced::Element<Message> = pane_grid::PaneGrid::new(&state.panes, move |pane, group_data, _maximized| {
            let body: iced::Element<Message> = group_data
                .active()
                .map(|id| content(pane, id))
                .unwrap_or_else(|| iced::widget::text("").into());

            let a = Rc::clone(&a_content);
            let tabs = TabRowWidget::new(group_data, move |ev| {
                let action = match ev {
                    TabEvent::Select(id) => DockAction::TabSelect(pane, id),
                    TabEvent::Close(id)  => DockAction::TabClose(pane, id),
                    TabEvent::Reorder { from, to } => DockAction::TabReorder { pane, from, to },
                };
                (a.as_ref())(action)
            });
            // No on_title_drag set — non-tab area is pane_grid's drag pick area.

            pane_grid::Content::new(body)
                .title_bar(pane_grid::TitleBar::new(tabs))
        })
        .on_click(move |p| (a_click.as_ref())(DockAction::PaneClicked(p)))
        .on_drag(move |e| (a_drag.as_ref())(DockAction::PaneDragged(e)))
        .on_resize(5.0, move |e| (a_resize.as_ref())(DockAction::PaneResized(e)))
        .width(iced::Length::Fill)
        .height(iced::Length::Fill)
        .spacing(spacing)
        .into();

        if let Some(hint_pos) = drag_hint {
            let overlay = HintOverlay { state, hint_pos, spacing };
            iced_widget::stack![grid, iced::Element::new(overlay)]
                .width(iced::Length::Fill)
                .height(iced::Length::Fill)
                .into()
        } else {
            grid
        }
    }
}

// ── FloatingDockWidget ────────────────────────────────────────────────────────

/// Widget for a detached (floating, borderless) dock group window.
///
/// Renders a tab row at the top and the active dock's content below.
/// Pressing the non-tab title area emits `FloatAction::StartWindowDrag` so the
/// caller can initiate an OS-native window drag via `iced_runtime::window::drag`.
pub struct FloatingDockWidget<'a, Message> {
    group_data: &'a DockGroupData,
    content: Box<dyn Fn(DockId) -> iced::Element<'a, Message> + 'a>,
    on_action: Box<dyn Fn(FloatAction) -> Message + 'a>,
}

impl<'a, Message: Clone + 'a> FloatingDockWidget<'a, Message> {
    pub fn new(
        group_data: &'a DockGroupData,
        on_action: impl Fn(FloatAction) -> Message + 'a,
    ) -> Self {
        Self {
            group_data,
            content: Box::new(|id| iced::widget::text(id.to_string()).into()),
            on_action: Box::new(on_action),
        }
    }

    pub fn content(
        mut self,
        f: impl Fn(DockId) -> iced::Element<'a, Message> + 'a,
    ) -> Self {
        self.content = Box::new(f);
        self
    }
}

impl<'a, Message: Clone + 'a> From<FloatingDockWidget<'a, Message>> for iced::Element<'a, Message> {
    fn from(w: FloatingDockWidget<'a, Message>) -> Self {
        use std::rc::Rc;

        let FloatingDockWidget { group_data, content, on_action } = w;

        let on_action: Rc<dyn Fn(FloatAction) -> Message + 'a> = Rc::from(on_action);
        let a_tab   = Rc::clone(&on_action);
        let a_title = Rc::clone(&on_action);

        let tab_row = TabRowWidget::new(group_data, move |ev| {
            (a_tab.as_ref())(FloatAction::Tab(ev))
        })
        .on_title_drag(move || (a_title.as_ref())(FloatAction::StartWindowDrag));

        let body: iced::Element<Message> = group_data
            .active()
            .map(|id| content(id))
            .unwrap_or_else(|| iced::widget::text("").into());

        iced_widget::column![
            iced::Element::from(tab_row),
            body,
        ]
        .width(iced::Length::Fill)
        .height(iced::Length::Fill)
        .into()
    }
}

// ── HintOverlay ───────────────────────────────────────────────────────────────

/// Transparent overlay widget drawn on top of `DockWidget` to show where a
/// floating window would re-attach (the pane half closest to the hint cursor).
struct HintOverlay<'a> {
    state: &'a DockState,
    hint_pos: iced::Point,
    spacing: f32,
}

impl<'a, Message, Theme, Renderer> iced_core::Widget<Message, Theme, Renderer>
    for HintOverlay<'a>
where
    Renderer: iced_core::Renderer,
{
    fn size(&self) -> iced_core::Size<iced::Length> {
        iced_core::Size::new(iced::Length::Fill, iced::Length::Fill)
    }

    fn layout(
        &mut self,
        _tree: &mut iced_core::widget::Tree,
        _renderer: &Renderer,
        limits: &iced_core::layout::Limits,
    ) -> iced_core::layout::Node {
        iced_core::layout::Node::new(limits.max())
    }

    fn draw(
        &self,
        _tree: &iced_core::widget::Tree,
        renderer: &mut Renderer,
        _theme: &Theme,
        _style: &iced_core::renderer::Style,
        layout: iced_core::Layout<'_>,
        _cursor: iced_core::mouse::Cursor,
        _viewport: &iced_core::Rectangle,
    ) {
        let bounds = layout.bounds();
        let regions = self.state.panes.layout().pane_regions(
            self.spacing, 0.0, bounds.size(),
        );

        // Convert window-relative hint_pos to grid-relative coords.
        let rel = iced_core::Point::new(
            self.hint_pos.x - bounds.x,
            self.hint_pos.y - bounds.y,
        );

        for (_, region) in &regions {
            if !region.contains(rel) {
                continue;
            }

            let cx = region.x + region.width  / 2.0;
            let cy = region.y + region.height / 2.0;

            let highlight = if (rel.x - cx).abs() > (rel.y - cy).abs() {
                // Vertical split — left or right half
                if rel.x < cx {
                    iced_core::Rectangle {
                        x: bounds.x + region.x,
                        y: bounds.y + region.y,
                        width:  region.width  / 2.0,
                        height: region.height,
                    }
                } else {
                    iced_core::Rectangle {
                        x: bounds.x + region.x + region.width / 2.0,
                        y: bounds.y + region.y,
                        width:  region.width  / 2.0,
                        height: region.height,
                    }
                }
            } else {
                // Horizontal split — top or bottom half
                if rel.y < cy {
                    iced_core::Rectangle {
                        x: bounds.x + region.x,
                        y: bounds.y + region.y,
                        width:  region.width,
                        height: region.height / 2.0,
                    }
                } else {
                    iced_core::Rectangle {
                        x: bounds.x + region.x,
                        y: bounds.y + region.y + region.height / 2.0,
                        width:  region.width,
                        height: region.height / 2.0,
                    }
                }
            };

            renderer.fill_quad(
                iced_core::renderer::Quad {
                    bounds: highlight,
                    ..iced_core::renderer::Quad::default()
                },
                iced_core::Background::Color(iced_core::Color {
                    r: 0.15,
                    g: 0.55,
                    b: 1.0,
                    a: 0.35,
                }),
            );
            break;
        }
    }
}
