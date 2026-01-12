use crate::graph::GraphDynamicInstancesStorage;

pub mod casters;
pub mod nodes;
pub mod types;

pub fn create_storage() -> GraphDynamicInstancesStorage {
    use nodes::*;
    use types::*;
    use casters::*;

    let mut storage = GraphDynamicInstancesStorage::default();

    storage.nodes.register::<ScalarMathNode>();
    storage.nodes.register::<VectorMathNode>();
    storage.nodes.register::<ClampNode>();
    storage.nodes.register::<StepNode>();
    storage.nodes.register::<SmoothStepNode>();
    storage.nodes.register::<SplitComponentsNode>();
    storage.nodes.register::<CombineComponentsNode>();

    storage.types.register::<F32Type>();
    storage.types.register::<Vec2FType>();

    storage.casters.register::<F32ToVec2FCaster>();
    storage.casters.register::<Vec2FToF32Caster>();

    storage
}
