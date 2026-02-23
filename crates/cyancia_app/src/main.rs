// use crate::main_view::MainView;

// mod input_manager;
// mod main_view;

use cyancia_runtime::Runtime;

fn main() {
    tracing_subscriber::fmt().with_env_filter("info").init();

    Runtime::default().run();
}
