use iced::{widget::center, Element, Task};
use iced_widget::pane_grid;
use cyancia_dock::{DockAction, DockGroupData, DockId, DockState, DockWidget};

fn main() -> iced::Result {
    iced::application(App::default, App::update, App::view)
        .title("Docking Test")
        .theme(iced::Theme::Dark)
        .run()
}

fn dock(s: &'static str) -> DockId {
    DockId::from(s)
}

#[derive(Debug, Clone)]
enum Message {
    Dock(DockAction),
}

struct App {
    state: DockState,
}

impl Default for App {
    fn default() -> Self {
        let left = DockGroupData::with_docks([dock("Properties"), dock("Timeline"), dock("Layers")]);
        let (mut state, left_pane) = DockState::new(left);

        let right = DockGroupData::with_docks([dock("Viewport"), dock("Assets"), dock("Console")]);
        state.split(pane_grid::Axis::Vertical, left_pane, right);

        App { state }
    }
}

impl App {
    fn update(&mut self, msg: Message) -> Task<Message> {
        match msg {
            Message::Dock(action) => self.state.update(action).map(Message::Dock),
        }
    }

    fn view(&self) -> Element<'_, Message> {
        DockWidget::new(&self.state, Message::Dock)
            .content(|_pane, dock_id| {
                center(iced::widget::text(dock_id.to_string()).size(20)).into()
            })
            .into()
    }
}
