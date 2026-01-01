use crate::main_view::MainView;

mod input_manager;
mod main_view;

fn main() {
    tracing_subscriber::fmt().with_env_filter("info").init();

    iced::application(MainView::new, MainView::update, MainView::view)
        .subscription(MainView::subscription)
        .run()
        .unwrap();
}
