use crate::graph::GraphDynamicInstancesStorage;

pub mod nodes;
pub mod types;

pub fn create_storage() -> GraphDynamicInstancesStorage {
    use nodes::*;
    use types::*;

    let mut storage = GraphDynamicInstancesStorage::default();

    storage.creators.register::<UnaryScalarMathNode>();
    storage.creators.register::<UnaryVectorMathNode>();
    storage.creators.register::<BinaryScalarMathNode>();
    storage.creators.register::<BinaryVectorMathNode>();

    storage.types.register::<F32Type>();
    storage.types.register::<I32Type>();
    storage.types.register::<U32Type>();
    storage.types.register::<Vec2FType>();

    storage
}
