use cyancia_brush::editor::BrushEditorView;
use cyancia_id::Id;
use cyancia_windows::WindowManager;
use iced::Theme;

use crate::main_view::MainView;

mod main_view;

fn main() {
    tracing_subscriber::fmt().with_env_filter("info").init();

    iced::daemon(
        || {
            let mut instance = WindowManager::<Theme, iced_wgpu::Renderer>::new();
            let main_view = MainView::new();
            instance.register(BrushEditorView::new(main_view.assets.clone()));
            instance.register(main_view);
            let task = instance.open_view(Id::from_str("main_view"));
            (instance, task.discard())
        },
        WindowManager::update,
        WindowManager::view,
    )
    .subscription(WindowManager::subscription)
    .run()
    .unwrap();
}
