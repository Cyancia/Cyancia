use crate::graph::GraphDynamicInstancesStorage;

pub mod nodes;

pub fn extended_storage() -> GraphDynamicInstancesStorage {
    use nodes::*;

    let mut storage = GraphDynamicInstancesStorage::default();
    storage.nodes.register::<CurveNode>();
    storage.nodes.register::<RandomNode>();
    storage
}
