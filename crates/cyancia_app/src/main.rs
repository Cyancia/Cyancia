use cyancia_windows::WindowManager;
use iced::Theme;

fn main() {
    tracing_subscriber::fmt().with_env_filter("info").init();

    iced::daemon(
        WindowManager::<Theme>::boot,
        WindowManager::update,
        WindowManager::view,
    )
    .subscription(WindowManager::subscription)
    .run()
    .unwrap();
}
