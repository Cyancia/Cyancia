use std::{
    fmt::Debug,
    ops::Deref,
    sync::{Arc, OnceLock},
};

#[derive(Debug)]
pub struct GlobalInstance<T>(OnceLock<Arc<T>>);

impl<T> Deref for GlobalInstance<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        self.0.get().expect(&format!(
            "GlobalInstance {} is not initialized",
            std::any::type_name::<T>()
        ))
    }
}

impl<T> GlobalInstance<T> {
    pub const fn new() -> Self {
        Self(OnceLock::new())
    }

    pub fn init(&self, value: T) {
        match self.0.set(Arc::new(value)) {
            Ok(_) => {}
            Err(_) => panic!(
                "GlobalInstance {} is already initialized",
                std::any::type_name::<T>()
            ),
        }
    }

    pub fn clone_arc(&self) -> Arc<T> {
        self.0
            .get()
            .expect(&format!(
                "GlobalInstance {} is not initialized",
                std::any::type_name::<T>()
            ))
            .clone()
    }
}
