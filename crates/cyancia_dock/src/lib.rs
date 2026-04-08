pub mod dock;
pub mod group;
pub mod style;
pub mod state;

pub use dock::{DockAction, DockId};
pub use group::{DockGroupData, TabRowWidget};
pub use state::DockState;
pub use style::{DockCatalog, DockStyle, DockStatus, TabBarStyle, TabStyle};

use iced_widget::pane_grid;

// ── DockWidget ────────────────────────────────────────────────────────────────

/// Top-level widget that renders the docking system backed by `PaneGrid`.
///
/// ```rust,no_run
/// # use cyancia_dock::{DockState, DockWidget, DockAction, DockId};
/// # use iced_widget::pane_grid;
/// # fn example<'a>(state: &'a DockState) -> iced::Element<'a, DockAction> {
/// DockWidget::new(state, std::convert::identity)
///     .content(|_pane, dock_id| iced::widget::text(dock_id.to_string()).into())
///     .into()
/// # }
/// ```
pub struct DockWidget<'a, Message> {
    state: &'a DockState,
    content: Box<dyn Fn(pane_grid::Pane, DockId) -> iced::Element<'a, Message> + 'a>,
    on_action: Box<dyn Fn(DockAction) -> Message + 'a>,
    spacing: f32,
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
}

impl<'a, Message: Clone + 'a> From<DockWidget<'a, Message>> for iced::Element<'a, Message> {
    fn from(w: DockWidget<'a, Message>) -> Self {
        use std::rc::Rc;

        let DockWidget { state, content, on_action, spacing } = w;

        // Share on_action across the multiple closures that pane_grid needs.
        let on_action: Rc<dyn Fn(DockAction) -> Message + 'a> = Rc::from(on_action);
        let a_content = Rc::clone(&on_action);
        let a_click   = Rc::clone(&on_action);
        let a_drag    = Rc::clone(&on_action);
        let a_resize  = Rc::clone(&on_action);

        pane_grid::PaneGrid::new(&state.panes, move |pane, group_data, _maximized| {
            let body: iced::Element<Message> = group_data
                .active()
                .map(|id| content(pane, id))
                .unwrap_or_else(|| iced::widget::text("").into());

            let a = Rc::clone(&a_content);
            let tabs = TabRowWidget::new(pane, group_data, move |action| (a.as_ref())(action));

            pane_grid::Content::new(body)
                .title_bar(pane_grid::TitleBar::new(tabs))
        })
        .on_click(move |p| (a_click.as_ref())(DockAction::PaneClicked(p)))
        .on_drag(move |e| (a_drag.as_ref())(DockAction::PaneDragged(e)))
        .on_resize(5.0, move |e| (a_resize.as_ref())(DockAction::PaneResized(e)))
        .width(iced::Length::Fill)
        .height(iced::Length::Fill)
        .spacing(spacing)
        .into()
    }
}
