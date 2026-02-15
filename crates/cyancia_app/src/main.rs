use cyancia_brush::editor::BrushEditorView;
use cyancia_id::Id;
use cyancia_windows::WindowManager;
use iced::Theme;

mod main_view;

fn main() {
    tracing_subscriber::fmt().with_env_filter("info").init();

    iced::daemon(
        || {
            let mut instance = WindowManager::<Theme, iced_wgpu::Renderer>::new();
            instance.register::<main_view::MainView>();
            instance.register::<BrushEditorView>();
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
