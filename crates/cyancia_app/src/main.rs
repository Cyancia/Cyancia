use std::sync::Arc;

use crate::main_view::MainView;

mod dock;
mod main_view;

use cyancia_actions::ActionPlugin;
use cyancia_assets::{
    AssetsPlugin,
    bundle::{ErasedAssetBundle, directory::AssetDirectory, standard::StandardAssetBundle},
};
use cyancia_brush::{BrushPlugin, editor::BrushEditor};
use cyancia_bucket_tool::BucketPlugin;
use cyancia_canvas::CanvasPlugin;
use cyancia_image::ImagePlugin;
use cyancia_input::InputPlugin;
use cyancia_render::RenderPlugin;
use cyancia_runtime::{Application, service::RenderContext, windows::WindowCommandBuffer};
use cyancia_selection_tool::SelectionPlugin;
use cyancia_shader_graph::ShaderGraphPlugin;
use cyancia_tools::ToolsPlugin;
use cyancia_transform_tool::FreeTransformPlugin;
use cyancia_undo::UndoPlugin;

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter("info,wgpu_hal=warn,iced_winit=warn,iced_wgpu=warn")
        .init();

    log::info!("Running at {}", std::env::current_dir().unwrap().display());

    let mut app = Application::default();
    let mut asset_bundles = Vec::<Arc<dyn ErasedAssetBundle>>::new();
    asset_bundles.push(Arc::new(
        AssetDirectory::new("assets/builtin_assets").unwrap(),
    ));

    {
        let (standard_bundles, errs) = StandardAssetBundle::scan_bundles("assets");
        log::info!(
            "Loaded {} csb bundles with {} errors",
            standard_bundles.len(),
            errs.len()
        );
        for err in errs {
            log::error!("Error loading asset bundle: {}", err);
        }
        for bundle in &standard_bundles {
            log::info!("Loaded asset bundle: {}", bundle.path().display());
        }
        asset_bundles.extend(
            standard_bundles
                .into_iter()
                .map(|b| Arc::new(b) as Arc<dyn ErasedAssetBundle>),
        );
    }

    app.add_service::<RenderContext>()
        .add_service::<WindowCommandBuffer>()
        .add_plugin(AssetsPlugin {
            asset_root: "assets".into(),
            bundles: asset_bundles,
        })
        .add_plugin(UndoPlugin)
        .add_plugin(RenderPlugin)
        .add_plugin(ShaderGraphPlugin)
        .add_plugin(ToolsPlugin)
        .add_plugin(ImagePlugin)
        .add_plugin(CanvasPlugin)
        .add_plugin(InputPlugin)
        .add_plugin(BrushPlugin)
        .add_plugin(BucketPlugin)
        .add_plugin(SelectionPlugin)
        .add_plugin(FreeTransformPlugin)
        .add_plugin(ActionPlugin);
    app.build_plugins();

    {
        let mut rt = app.runtime_mut();

        rt.window_manager_mut().set_root_view::<MainView>();
        rt.window_manager_mut().register_view::<MainView>();
        rt.window_manager_mut().register_view::<BrushEditor>();
    }

    app.run().unwrap();
}
