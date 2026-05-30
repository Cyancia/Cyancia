use cyancia_brush::editor::BrushEditor;
use cyancia_view::{View, ViewId};
use gpui::{
    App, AppContext, Context, Entity, IntoElement, Render, Window, WindowHandle, WindowOptions,
};
use gpui_component::Root;

pub struct BrushEditorView {
    editor: Entity<BrushEditor>,
}

impl View for BrushEditorView {
    fn id() -> ViewId {
        ViewId::new("brush_editor")
    }

    fn open(cx: &mut App) -> anyhow::Result<WindowHandle<Root>> {
        cx.open_window(
            WindowOptions {
                ..Default::default()
            },
            |window, cx| {
                let editor_view = cx.new(|cx| BrushEditorView::new(window, cx));
                let root = cx.new(|cx| Root::new(editor_view, window, cx));
                root
            },
        )
    }
}

impl BrushEditorView {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let editor = cx.new(|cx| BrushEditor::new(window, cx));
        Self { editor }
    }
}

impl Render for BrushEditorView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.editor.clone()
    }
}
