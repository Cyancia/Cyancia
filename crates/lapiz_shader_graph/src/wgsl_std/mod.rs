use bevy_color::{Oklcha, Srgba};
use iced_core::Color;

use crate::graph::{GraphData, node::GraphNodeRegistry, variable::GraphTypeRegistry};

pub mod casters;
pub mod nodes;
pub mod types;

pub(crate) fn themed_color(name: &str, is_dark: bool) -> Color {
    let hash = name.bytes().fold(0_u32, |hash, byte| {
        (hash << 5).wrapping_sub(hash) + byte as u32
    });
    let chroma = 0.05 + (hash as f32 / u32::MAX as f32) * 0.1;
    let hue = (hash % 360) as f32;
    let lightness = if is_dark { 0.4 } else { 0.7 };
    let color = Srgba::from(Oklcha::new(lightness, chroma, hue, 1.0));
    Color::from_rgba(
        color.red.clamp(0.0, 1.0),
        color.green.clamp(0.0, 1.0),
        color.blue.clamp(0.0, 1.0),
        color.alpha.clamp(0.0, 1.0),
    )
}

pub fn builtin_nodes<Data: GraphData>() -> GraphNodeRegistry<Data> {
    use nodes::*;

    let mut nodes = GraphNodeRegistry::with_capacity();

    nodes.register::<ScalarMathNode>();
    nodes.register::<VectorMathNode>();
    nodes.register::<RectMathNode>();
    nodes.register::<CompareNode>();
    nodes.register::<ScalarSelectNode>();
    nodes.register::<VectorSelectNode>();
    nodes.register::<ClampNode>();
    nodes.register::<RandomNode>();
    nodes.register::<StepNode>();
    nodes.register::<SmoothStepNode>();
    nodes.register::<SplitComponentsNode>();
    nodes.register::<CombineComponentsNode>();
    nodes.register::<SplitColorComponentsNode>();
    nodes.register::<CombineColorComponentsNode>();
    nodes.register::<GetPixelColorNode>();
    nodes.register::<ColorMixNode>();
    nodes.register::<TextureNode>();
    nodes.register::<TextureSizeNode>();
    nodes.register::<GraphFunctionNode>();
    nodes.register::<ExternalVariableNode>();
    nodes.register::<CurveNode>();
    nodes.register::<WhileNode>();

    nodes
}

pub fn builtin_types() -> GraphTypeRegistry {
    use casters::*;
    use types::*;

    let mut types = GraphTypeRegistry::default();

    types.register_type::<F32Type>();
    types.register_type::<I32Type>();
    types.register_type::<BoolType>();
    types.register_type::<Vec2FType>();
    types.register_type::<ColorType>();
    types.register_type::<TextureType>();
    types.register_type::<RectType>();

    types.register_caster::<F32ToVec2FCaster>();
    types.register_caster::<Vec2FToF32Caster>();
    types.register_caster::<BoolToI32Caster>();
    types.register_caster::<I32ToBoolCaster>();
    types.register_caster::<F32ToI32Caster>();
    types.register_caster::<I32ToF32Caster>();
    types.register_caster::<Vec2FToI32Caster>();
    types.register_caster::<I32ToVec2FCaster>();

    types
}
