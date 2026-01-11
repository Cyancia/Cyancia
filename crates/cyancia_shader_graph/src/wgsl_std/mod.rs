use crate::graph::GraphDynamicInstancesStorage;

pub mod casters;
pub mod nodes;
pub mod types;

pub fn create_storage() -> GraphDynamicInstancesStorage {
    use nodes::*;
    use types::*;
    use casters::*;

    let mut storage = GraphDynamicInstancesStorage::default();

    storage.creators.register::<ScalarMathNode>();
    storage.creators.register::<VectorMathNode>();
    storage.creators.register::<ClampNode>();
    storage.creators.register::<StepNode>();
    storage.creators.register::<SmoothStepNode>();
    storage.creators.register::<SplitComponentsNode>();
    storage.creators.register::<CombineComponentsNode>();

    storage.types.register::<F32Type>();
    storage.types.register::<Vec2FType>();

    storage.casters.register::<F32ToVec2FCaster>();
    storage.casters.register::<Vec2FToF32Caster>();

    storage
}
