use std::sync::Arc;

use cyancia_utils::wrapper;
use iced_core::{
    Element, Layout, Length, Point, Rectangle, Size, layout, mouse, renderer, widget, window,
};
use iced_widget::{pane_grid, space, stack};
use parse_display::Display;
use serde::Serialize;

use crate::{
    AttachInfo, DockState,
    group::{DockGroupData, TabRowWidget},
};

wrapper! {
    #[derive(Debug, Clone, PartialEq, Eq, Hash, Display, Serialize)]
    #[display("{0}")]
    pub DockId : Arc<str>
}

/// Events the docking system can emit.
#[derive(Debug, Clone)]
pub enum DockAction {
    Pane(PaneEvent),
    Tab(pane_grid::Pane, TabEvent),
}

#[derive(Debug, Clone)]
pub enum PaneEvent {
    Clicked(pane_grid::Pane),
    Dragged(pane_grid::DragEvent),
    Resized(pane_grid::ResizeEvent),
}

/// Low-level tab events emitted by `TabRowWidget` (no pane coupling).
///
/// `DockWidget` maps these back to `DockAction` variants. `FloatingDockWidget` wraps them
/// inside `FloatAction::Tab`.
#[derive(Debug, Clone)]
pub enum TabEvent {
    Select(DockId),
    Close(DockId),
    Reorder { from: usize, to: usize },
    Detach(DockId),
}

/// Actions emitted by `FloatingDockWidget`.
#[derive(Debug, Clone)]
pub enum FloatAction {
    Tab(TabEvent),
    /// User pressed the non-tab title area — the app should call `window::drag(id)`.
    StartWindowDrag,
}

// ── DockWidget ────────────────────────────────────────────────────────────────

/// Top-level widget that renders the docking system backed by `PaneGrid`.
pub struct DockWidget<'a, Message, Theme, Renderer> {
    state: &'a DockState,
    content:
        Option<Box<dyn Fn(pane_grid::Pane, DockId) -> Element<'a, Message, Theme, Renderer> + 'a>>,
    on_action: Box<dyn Fn(DockAction) -> Message + 'a>,
    spacing: f32,
    attach_info: Option<AttachInfo>,
}

impl<'a, Message: Clone + 'a, Theme, Renderer> DockWidget<'a, Message, Theme, Renderer> {
    pub fn new(state: &'a DockState, on_action: impl Fn(DockAction) -> Message + 'a) -> Self {
        Self {
            state,
            content: None,
            on_action: Box::new(on_action),
            spacing: 2.0,
            attach_info: None,
        }
    }

    pub fn content(
        mut self,
        f: impl Fn(pane_grid::Pane, DockId) -> Element<'a, Message, Theme, Renderer> + 'a,
    ) -> Self {
        self.content = Some(Box::new(f));
        self
    }

    pub fn spacing(mut self, s: f32) -> Self {
        self.spacing = s;
        self
    }

    pub fn attach_info(mut self, split_info: AttachInfo) -> Self {
        self.attach_info = Some(split_info);
        self
    }
}

impl<'a, Message, Theme, Renderer> From<DockWidget<'a, Message, Theme, Renderer>>
    for Element<'a, Message, Theme, Renderer>
where
    Message: Clone + 'a,
    Theme: iced_widget::container::Catalog
        + iced_widget::pane_grid::Catalog
        + crate::style::DockCatalog
        + 'a,
    Renderer: iced_core::Renderer + iced_core::text::Renderer + 'a,
{
    fn from(w: DockWidget<'a, Message, Theme, Renderer>) -> Self {
        use std::rc::Rc;

        let DockWidget {
            state,
            content,
            on_action,
            spacing,
            attach_info: drag_hint,
        } = w;

        let on_action = Rc::<dyn Fn(DockAction) -> Message>::from(on_action);
        let a_content = Rc::clone(&on_action);
        let a_click = Rc::clone(&on_action);
        let a_drag = Rc::clone(&on_action);
        let a_resize = Rc::clone(&on_action);

        let grid =
            pane_grid::PaneGrid::new(state.panes_state(), move |pane, group_data, _maximized| {
                let body = group_data
                    .active()
                    .and_then(|id| content.as_ref().map(|c| c(pane, id.clone())))
                    .unwrap_or_else(|| Element::new(space()));

                let a = Rc::clone(&a_content);
                let tabs = TabRowWidget::new(group_data, move |ev| {
                    (a.as_ref())(DockAction::Tab(pane, ev))
                });
                // No on_title_drag set — non-tab area is pane_grid's drag pick area.

                pane_grid::Content::new(body)
                    .title_bar(pane_grid::TitleBar::new(Element::new(tabs)))
            })
            .on_click(move |p| (a_click.as_ref())(DockAction::Pane(PaneEvent::Clicked(p))))
            .on_drag(move |e| (a_drag.as_ref())(DockAction::Pane(PaneEvent::Dragged(e))))
            .on_resize(5.0, move |e| {
                (a_resize.as_ref())(DockAction::Pane(PaneEvent::Resized(e)))
            })
            .width(Length::Fill)
            .height(Length::Fill)
            .spacing(spacing)
            .into();

        if let Some(split_info) = drag_hint {
            let overlay = PaneHintOverlay {
                state,
                attach_info: split_info,
                spacing,
            };
            iced_widget::stack![grid, Element::new(overlay)]
                .width(Length::Fill)
                .height(Length::Fill)
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
pub struct FloatingDockWidget<'a, Message, Theme, Renderer> {
    group_data: &'a DockGroupData,
    content: Option<Box<dyn Fn(DockId) -> Element<'a, Message, Theme, Renderer> + 'a>>,
    on_action: Box<dyn Fn(FloatAction) -> Message + 'a>,
    is_attaching: bool,
}

impl<'a, Message, Theme, Renderer> FloatingDockWidget<'a, Message, Theme, Renderer>
where
    Message: Clone + 'a,
{
    pub fn new(
        group_data: &'a DockGroupData,
        on_action: impl Fn(FloatAction) -> Message + 'a,
    ) -> Self {
        Self {
            group_data,
            content: None,
            on_action: Box::new(on_action),
            is_attaching: false,
        }
    }

    pub fn content(
        mut self,
        f: impl Fn(DockId) -> Element<'a, Message, Theme, Renderer> + 'a,
    ) -> Self {
        self.content = Some(Box::new(f));
        self
    }

    pub fn is_merging(mut self, attaching: bool) -> Self {
        self.is_attaching = attaching;
        self
    }
}

impl<'a, Message, Theme, Renderer> From<FloatingDockWidget<'a, Message, Theme, Renderer>>
    for Element<'a, Message, Theme, Renderer>
where
    Message: Clone + 'a,
    Theme: crate::style::DockCatalog + 'a,
    Renderer: iced_core::Renderer + iced_core::text::Renderer + 'a,
{
    fn from(w: FloatingDockWidget<'a, Message, Theme, Renderer>) -> Self {
        use std::rc::Rc;

        let FloatingDockWidget {
            group_data,
            content,
            on_action,
            is_attaching,
        } = w;

        let on_action: Rc<dyn Fn(FloatAction) -> Message + 'a> = Rc::from(on_action);
        let a_tab = Rc::clone(&on_action);
        let a_title_drag = Rc::clone(&on_action);

        let tab_row =
            TabRowWidget::new(group_data, move |ev| (a_tab.as_ref())(FloatAction::Tab(ev)))
                .on_title_drag(move || (a_title_drag.as_ref())(FloatAction::StartWindowDrag));

        let body = group_data
            .active()
            .and_then(|id| content.map(|c| c(id.clone())))
            .unwrap_or_else(|| Element::new(space()));

        let content = iced_widget::column![Element::from(tab_row), body]
            .width(Length::Fill)
            .height(Length::Fill);

        if is_attaching {
            stack![Element::new(WindowHintOverlay), content].into()
        } else {
            content.into()
        }
    }
}

const ATTACH_HINT_COLOR: iced_core::Color = iced_core::Color {
    r: 0.15,
    g: 0.55,
    b: 1.0,
    a: 0.35,
};

/// Transparent overlay widget drawn on top of `DockWidget` to show where a
/// floating window would re-attach (the pane half closest to the hint cursor).
struct PaneHintOverlay<'a> {
    state: &'a DockState,
    attach_info: AttachInfo,
    spacing: f32,
}

impl<'a, Message, Theme, Renderer> iced_core::Widget<Message, Theme, Renderer>
    for PaneHintOverlay<'a>
where
    Renderer: iced_core::Renderer,
{
    fn size(&self) -> Size<Length> {
        Size::new(Length::Fill, Length::Fill)
    }

    fn layout(
        &mut self,
        _tree: &mut widget::Tree,
        _renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        layout::Node::new(limits.max())
    }

    fn draw(
        &self,
        _tree: &widget::Tree,
        renderer: &mut Renderer,
        _theme: &Theme,
        _style: &renderer::Style,
        layout: Layout<'_>,
        _cursor: mouse::Cursor,
        _viewport: &Rectangle,
    ) {
        let bounds = layout.bounds();
        let regions =
            self.state
                .panes_state()
                .layout()
                .pane_regions(self.spacing, 0.0, bounds.size());

        let Some(region) = regions.get(&self.attach_info.target_pane()) else {
            return;
        };

        let highlight = match self.attach_info {
            AttachInfo::Split { result_edge, .. } => match result_edge {
                pane_grid::Edge::Left => iced_core::Rectangle {
                    x: bounds.x + region.x,
                    y: bounds.y + region.y,
                    width: region.width / 2.0,
                    height: region.height,
                },
                pane_grid::Edge::Right => iced_core::Rectangle {
                    x: bounds.x + region.x + region.width / 2.0,
                    y: bounds.y + region.y,
                    width: region.width / 2.0,
                    height: region.height,
                },
                pane_grid::Edge::Top => iced_core::Rectangle {
                    x: bounds.x + region.x,
                    y: bounds.y + region.y,
                    width: region.width,
                    height: region.height / 2.0,
                },
                pane_grid::Edge::Bottom => iced_core::Rectangle {
                    x: bounds.x + region.x,
                    y: bounds.y + region.y + region.height / 2.0,
                    width: region.width,
                    height: region.height / 2.0,
                },
            },
            AttachInfo::Merge { .. } => iced_core::Rectangle {
                x: bounds.x + region.x,
                y: bounds.y + region.y,
                width: region.width,
                height: region.height,
            },
        };

        renderer.fill_quad(
            iced_core::renderer::Quad {
                bounds: highlight,
                ..iced_core::renderer::Quad::default()
            },
            iced_core::Background::Color(ATTACH_HINT_COLOR),
        );
    }
}

struct WindowHintOverlay;

impl<Message, Theme, Renderer> iced_core::Widget<Message, Theme, Renderer> for WindowHintOverlay
where
    Renderer: iced_core::Renderer,
{
    fn size(&self) -> Size<Length> {
        Size::new(Length::Fill, Length::Fill)
    }

    fn layout(
        &mut self,
        _tree: &mut widget::Tree,
        _renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        layout::Node::new(limits.max())
    }

    fn draw(
        &self,
        _tree: &widget::Tree,
        renderer: &mut Renderer,
        _theme: &Theme,
        _style: &renderer::Style,
        layout: Layout<'_>,
        _cursor: mouse::Cursor,
        _viewport: &Rectangle,
    ) {
        renderer.fill_quad(
            iced_core::renderer::Quad {
                bounds: layout.bounds(),
                ..Default::default()
            },
            iced_core::Background::Color(ATTACH_HINT_COLOR),
        );
    }
}
