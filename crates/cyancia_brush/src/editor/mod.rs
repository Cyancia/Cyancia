use cyancia_id::Id;
use cyancia_shader_graph::{GraphRenderer, GraphTheme};
use cyancia_windows::{Window, WindowManagerShell, WindowView};
use iced_core::Element;
use iced_runtime::Task;
use iced_widget::space;

pub struct BrushEditorView {}

impl Default for BrushEditorView {
    fn default() -> Self {
        Self {}
    }
}

pub enum BrushEditorMessage {}

impl WindowView<GraphTheme, GraphRenderer> for BrushEditorView {
    type Message = BrushEditorMessage;

    fn id(&self) -> Id<Window> {
        Id::from_str("brush_editor")
    }

    fn view(&self) -> Element<'static, Self::Message, GraphTheme, GraphRenderer> {
        space().into()
    }

    fn update(
        &mut self,
        message: Self::Message,
        windows: &mut WindowManagerShell,
    ) -> Task<Self::Message> {
        Task::none()
    }
}
