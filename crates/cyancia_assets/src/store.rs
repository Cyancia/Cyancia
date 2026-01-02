use std::{
    any::{Any, TypeId},
    collections::{HashMap, hash_map::Entry},
    path::Path,
    sync::Arc,
};

use cyancia_id::Id;

use crate::{
    asset::Asset,
    loader::{AssetLoader, ErasedAssetLoader},
};

pub struct AssetLoaderRegistry {
    loaders: HashMap<&'static str, Arc<dyn ErasedAssetLoader>>,
}

impl AssetLoaderRegistry {
    pub fn new() -> Self {
        Self {
            loaders: HashMap::new(),
        }
    }

    pub fn register<L: AssetLoader + Default>(&mut self) {
        let loader = Arc::new(L::default());
        for &ext in L::file_extensions() {
            self.loaders.insert(ext, loader.clone());
        }
    }

    pub fn get(&self, ext: &str) -> Option<Arc<dyn ErasedAssetLoader>> {
        self.loaders.get(ext).cloned()
    }
}

pub struct AssetRegistry {
    stores: HashMap<TypeId, Box<dyn Any + Send + Sync + 'static>>,
}

impl AssetRegistry {
    pub fn new(root: impl AsRef<Path>, loaders: &AssetLoaderRegistry) -> Self {
        let mut assets = Self {
            stores: HashMap::new(),
        };

        asset_loading::load_all_assets(&mut assets, loaders, root.as_ref());

        assets
    }

    pub fn store<T: Asset>(&self) -> &AssetStore<T> {
        self.stores
            .get(&TypeId::of::<T>())
            .expect(&format!(
                "Store of type {} doesn't exist.",
                std::any::type_name::<T>()
            ))
            .downcast_ref::<AssetStore<T>>()
            .unwrap()
    }

    pub fn store_mut<T: Asset>(&mut self) -> &mut AssetStore<T> {
        self.stores
            .get_mut(&TypeId::of::<T>())
            .expect(&format!(
                "Store of type {} doesn't exist.",
                std::any::type_name::<T>()
            ))
            .downcast_mut::<AssetStore<T>>()
            .unwrap()
    }

    pub fn asset<T: Asset>(&self, id: Id<T>) -> Option<Arc<T>> {
        self.store::<T>().get(id)
    }

    pub fn init_store<T: Asset>(&mut self) {
        match self.stores.entry(TypeId::of::<T>()) {
            Entry::Occupied(_) => {}
            Entry::Vacant(e) => {
                e.insert(Box::new(AssetStore::<T>::new()));
            }
        }
    }
}

mod asset_loading {
    use std::{fs::read_dir, path::PathBuf};

    use cyancia_id::UntypedId;

    use super::*;

    pub(super) fn load_all_assets(
        assets: &mut AssetRegistry,
        loaders: &AssetLoaderRegistry,
        root: &Path,
    ) {
        let mut counter = 0;
        match load_folder(assets, loaders, root, &mut counter) {
            Ok(_) => {
                log::info!(
                    "Successfully loaded {} assets from {}",
                    counter,
                    root.display()
                );
            }
            Err(e) => {
                log::error!("Error loading assets from root {}: {}", root.display(), e);
            }
        }
    }

    fn load_folder(
        assets: &mut AssetRegistry,
        loaders: &AssetLoaderRegistry,
        path: &Path,
        counter: &mut u32,
    ) -> Result<(), std::io::Error> {
        for entry in read_dir(path)? {
            let entry = match entry {
                Ok(e) => e,
                Err(e) => {
                    log::error!("Error reading directory entry {}: {}", e, path.display());
                    continue;
                }
            };

            let path = entry.path();
            if path.is_dir() {
                match load_folder(assets, loaders, &path, counter) {
                    Ok(_) => {}
                    Err(e) => log::error!("Error loading directory {}: {}", path.display(), e),
                };
            } else if path.is_file() {
                match load_file(assets, loaders, &path) {
                    Ok(_) => {
                        log::info!("Loaded file: {}", path.display());
                        *counter += 1;
                    }
                    Err(e) => log::error!("Error loading file {}: {}", path.display(), e),
                };
            } else {
                log::warn!("Skipping non-file, non-directory: {}", path.display());
            }
        }

        Ok(())
    }

    #[derive(Debug, thiserror::Error)]
    enum LoadFileError {
        #[error(transparent)]
        Io(#[from] std::io::Error),
        #[error("Unknown file extension for path: {0}")]
        UnknownExtension(PathBuf),
        #[error("Error loading asset {0}: {1}")]
        Loader(PathBuf, Box<dyn std::error::Error>),
    }

    fn load_file(
        assets: &mut AssetRegistry,
        loaders: &AssetLoaderRegistry,
        path: &Path,
    ) -> Result<(), LoadFileError> {
        let ext = path
            .extension()
            .and_then(|s| s.to_str())
            .ok_or_else(|| LoadFileError::UnknownExtension(path.to_path_buf()))?;
        let loader = loaders
            .get(ext)
            .ok_or_else(|| LoadFileError::UnknownExtension(path.to_path_buf()))?;
        let mut file = std::fs::File::open(path)?;
        let asset = loader
            .read(&mut file)
            .map_err(|e| LoadFileError::Loader(path.to_path_buf(), e))?;
        loader.insert_asset(UntypedId::random((*asset).type_id()), asset, assets);

        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct AssetStore<T: Asset> {
    assets: HashMap<Id<T>, Arc<T>>,
}

impl<T: Asset> AssetStore<T> {
    pub fn new() -> Self {
        Self {
            assets: HashMap::new(),
        }
    }

    pub fn get(&self, id: Id<T>) -> Option<Arc<T>> {
        self.assets.get(&id).cloned()
    }

    pub fn insert(&mut self, id: Id<T>, asset: Arc<T>) {
        self.assets.insert(id, asset);
    }

    pub fn into_map(self) -> HashMap<Id<T>, Arc<T>> {
        self.assets
    }
}
