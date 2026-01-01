use cyancia_assets::store::{AssetLoaderRegistry, AssetRegistry};
use cyancia_id::Id;
use cyancia_input::action::{
    ActionCollection, ActionManifest, ActionManifestLoader, matcher::ActionMatcher,
};
use iced_core::keyboard::key::Code;

fn main() {
    tracing_subscriber::fmt::init();

    let mut loaders = AssetLoaderRegistry::new();
    loaders.register::<ActionManifestLoader>();

    let assets = AssetRegistry::new("assets", &loaders);
    let manifests = assets.store::<ActionManifest>();
    let mut matcher = ActionMatcher::new(ActionCollection::new(manifests.clone()));

    matcher.key_pressed(Code::Space);
    assert_eq!(
        matcher.current_action().map(|a| a.0),
        Some(Id::from_str("canvas_pan_action"))
    );
    matcher.key_pressed(Code::ControlLeft);
    assert_eq!(
        matcher.current_action().map(|a| a.0),
        Some(Id::from_str("canvas_zoom_action"))
    );
    matcher.key_released(Code::Space);
    assert_eq!(matcher.current_action().map(|a| a.0), None);
}
