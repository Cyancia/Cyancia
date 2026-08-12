use lapiz_runtime::{Application, plugin::Plugin};
use lapiz_tools::ToolsAppExt;

use crate::free::FreeTransformTool;

pub mod free;

pub struct FreeTransformPlugin;

impl Plugin for FreeTransformPlugin {
    fn build(&self, app: &mut Application) {
        app.runtime_mut()
            .services_mut()
            .add_tool_function::<FreeTransformTool>();
    }
}
