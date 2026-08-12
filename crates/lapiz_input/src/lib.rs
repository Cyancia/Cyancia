use lapiz_runtime::{Application, plugin::Plugin};

use crate::key::KeyboardState;

pub mod key;
pub mod mouse;

pub struct InputPlugin;

impl Plugin for InputPlugin {
    fn build(&self, app: &mut Application) {
        app.add_service::<KeyboardState>();
    }

    fn finish(&self, _app: &mut Application) {}
}
