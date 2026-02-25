use crate::main_view::MainView;

mod input_manager;
mod main_view;

use cyancia_canvas::CanvasPlugin;
use cyancia_image::ImagePlugin;
use cyancia_render::RenderPlugin;
use cyancia_runtime::{Application, ApplicationProgram, Runtime, windows::WindowManager};

fn main() {
    tracing_subscriber::fmt().with_env_filter("info").init();

    ApplicationProgram::new(|| {
        let mut app = Application::default();
        app.add_plugin(RenderPlugin)
            .add_plugin(ImagePlugin)
            .add_plugin(CanvasPlugin);
        let main_view = futures::executor::block_on(MainView::new(app.runtime()));
        app.window_manager_mut().register_view(main_view);

        app
    })
    .run()
    .unwrap();
}
