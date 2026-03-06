use crate::graph::GraphDynamicInstancesStorage;

pub mod casters;
pub mod nodes;
pub mod types;

pub fn std_storage() -> GraphDynamicInstancesStorage {
    use casters::*;
    use nodes::*;
    use types::*;

    let mut storage = GraphDynamicInstancesStorage::default();

    storage.nodes.register::<ScalarMathNode>();
    storage.nodes.register::<VectorMathNode>();
    storage.nodes.register::<ClampNode>();
    storage.nodes.register::<StepNode>();
    storage.nodes.register::<SmoothStepNode>();
    storage.nodes.register::<SplitComponentsNode>();
    storage.nodes.register::<CombineComponentsNode>();
    storage.nodes.register::<SplitColorComponentsNode>();
    storage.nodes.register::<CombineColorComponentsNode>();
    storage.nodes.register::<GetPixelColorNode>();
    storage.nodes.register::<ColorMixNode>();

    storage.types.register::<F32Type>();
    storage.types.register::<Vec2FType>();
    storage.types.register::<ColorType>();
    storage.types.register::<TextureType>();

    storage.casters.register::<F32ToVec2FCaster>();
    storage.casters.register::<Vec2FToF32Caster>();

    storage
}
