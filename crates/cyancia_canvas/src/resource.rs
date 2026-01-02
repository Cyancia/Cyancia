use std::{
    any::{Any, TypeId},
    collections::HashMap,
    sync::Arc,
};

use cyancia_utils::wrapper;
use parking_lot::RwLock;

wrapper! {
    #[derive(Debug)]
    pub CanvasResource<T> : Arc<RwLock<T>>
}

impl<T> Clone for CanvasResource<T> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

pub struct CanvasResources {
    resources: HashMap<TypeId, Box<dyn Any + Send + Sync>>,
}

impl CanvasResources {
    pub fn new() -> Self {
        Self {
            resources: HashMap::new(),
        }
    }

    pub fn resource<T: Send + Sync + 'static>(&self) -> CanvasResource<T> {
        self.resources
            .get(&TypeId::of::<T>())
            .unwrap()
            .downcast_ref::<CanvasResource<T>>()
            .unwrap()
            .clone()
    }

    pub fn set<T>(&mut self, resource: T)
    where
        T: Send + Sync + 'static,
    {
        self.resources.insert(
            TypeId::of::<T>(),
            Box::new(CanvasResource::new(Arc::new(RwLock::new(resource)))),
        );
    }
}
