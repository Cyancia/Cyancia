use std::sync::Arc;

use cyancia_assets::{
    AssetAppExt,
    bundle::{directory::AssetDirectory, standard::StandardAssetBundle},
    loader::AssetRegistryBuilder,
};
use cyancia_view::{View, ViewAppExt, ViewManager};
use gpui::{AppContext, WindowOptions};
use gpui_component::Root;
use tracing_subscriber::fmt::format::FmtSpan;

use crate::{brush_editor_view::BrushEditorView, main_view::MainView};

mod brush_editor_view;
mod dock;
mod main_view;

fn main() {
    #[cfg(debug_assertions)]
    unsafe {
        // On windows platforms, if we enable direct composition, renderdoc will failed to launch it.
        // Not sure why this would happen, probably because we are using wgpu inside a gpui app.
        // TODO: Try this after gpui is using wgpu for rendering on windows as well.
        std::env::set_var("GPUI_DISABLE_DIRECT_COMPOSITION", "1");
    }

    tracing_subscriber::fmt()
        .with_env_filter("info,wgpu_hal=warn,iced_winit=warn,iced_wgpu=warn")
        .with_span_events(FmtSpan::CLOSE)
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
            cyancia_theme::init(cx);
            cyancia_view::init(cx);
            cyancia_bucket_tool::init(cx);

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
            cyancia_theme::finish(cx);

            let vm = cx.global_mut::<ViewManager>();
            vm.set_main_view(MainView::id());

            cx.register_view::<MainView>();
            cx.register_view::<BrushEditorView>();
            cx.open_view(MainView::id());
        });
}
