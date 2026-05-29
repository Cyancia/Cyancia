use std::sync::Arc;

use cyancia_assets::{
    AssetAppExt,
    bundle::{directory::AssetDirectory, standard::StandardAssetBundle},
    loader::AssetRegistryBuilder,
};
use gpui::{AppContext, WindowOptions};
use gpui_component::Root;

use crate::main_view::MainView;

mod dock;
mod main_view;

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter("info,wgpu_hal=warn,iced_winit=warn,iced_wgpu=warn")
        .init();

    log::info!("Running at {}", std::env::current_dir().unwrap().display());

    gpui_platform::application()
        .with_assets(gpui_component_assets::Assets)
        .run(|cx| {
            gpui_component::init(cx);

            cyancia_assets::init(cx);
            cx.global_mut::<AssetRegistryBuilder>()
                .set_root("assets".into());
            cyancia_render::init(cx);
            cyancia_actions::init(cx);
            cyancia_tools::init(cx);
            cyancia_brush::init(cx);
            cyancia_canvas::init(cx);
            cyancia_image::init(cx);
            cyancia_shader_graph::init(cx);

            {
                cx.add_asset_bundle(Arc::new(AssetDirectory::new("assets/builtin_assets")));
                let (standard_bundles, errs) = StandardAssetBundle::scan_bundles("assets");
                log::info!(
                    "Loaded {} csb bundles with {} errors",
                    standard_bundles.len(),
                    errs.len()
                );
                for err in errs {
                    log::error!("Error loading asset bundle: {}", err);
                }
                for bundle in standard_bundles {
                    log::info!("Loaded asset bundle: {}", bundle.path().display());
                    cx.add_asset_bundle(Arc::new(bundle));
                }
            }

            cyancia_assets::finish(cx);
            cyancia_actions::finish(cx);

            cx.open_window(
                WindowOptions {
                    titlebar: None,
                    ..Default::default()
                },
                |window, cx| {
                    let main_view = cx.new(|cx| MainView::new(window, cx));

                    cx.new(|cx| Root::new(main_view, window, cx))
                },
            );
        });
}
