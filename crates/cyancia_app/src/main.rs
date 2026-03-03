use std::sync::Arc;

use crate::main_view::MainView;

mod input_manager;
mod main_view;

use cyancia_actions::ActionPlugin;
use cyancia_assets::{
    AssetsPlugin,
    bundle::{ErasedAssetBundle, directory::AssetDirectory},
    store::AssetRegistry,
};
use cyancia_canvas::CanvasPlugin;
use cyancia_image::ImagePlugin;
use cyancia_input::InputPlugin;
use cyancia_render::RenderPlugin;
use cyancia_runtime::{
    Application, Runtime, Services,
    service::RenderContext,
    windows::{WindowManager, WindowView},
};
use cyancia_tools::ToolsPlugin;

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter("info,wgpu_hal=warn")
        .init();

    let mut app = Application::default();
    app.add_service::<RenderContext>();
    app.add_plugin(RenderPlugin)
        .add_plugin(AssetsPlugin {
            asset_root: "assets".into(),
            bundles: vec![Arc::new(AssetDirectory::new("assets/builtin_assets"))],
        })
        .add_plugin(ImagePlugin)
        .add_plugin(CanvasPlugin)
        .add_plugin(InputPlugin)
        .add_plugin(ToolsPlugin)
        .add_plugin(ActionPlugin);
    app.build_plugins();

    {
        let mut rt = app.runtime_mut();

        let main_view = MainView::new(rt.services());
        rt.window_manager_mut().set_root_view(main_view.id());
        rt.window_manager_mut().register_view(main_view);
    }

    app.run().unwrap();
}
