use std::{
    fs::File,
    path::{Path, PathBuf},
    sync::Arc,
};

use anyhow::Result;
use parking_lot::{Mutex, MutexGuard, RawMutex, lock_api::MappedMutexGuard};
use rusqlite::{Connection, OpenFlags};

pub mod image_props;
pub mod layer_tree;
pub mod tile_data;
pub use image_props::ImageProperties;
pub use layer_tree::LayerNode;

struct Inner {
    path: Option<PathBuf>,
    conn: Connection,
}

pub struct CyanArchive {
    inner: Arc<Mutex<Inner>>,
}

impl std::fmt::Debug for CyanArchive {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CyanArchive").finish()
    }
}

impl CyanArchive {
    pub fn new(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        if let Some(parent) = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            std::fs::create_dir_all(parent)?;
        }

        File::create_new(path)?;

        let conn = Connection::open(path)?;
        let archive = Self {
            inner: Arc::new(Mutex::new(Inner {
                path: Some(path.to_path_buf()),
                conn,
            })),
        };
        archive.initialize_tables()?;

        Ok(archive)
    }

    pub fn new_in_memory() -> Result<Self> {
        let archive = Self {
            inner: Arc::new(Mutex::new(Inner {
                path: None,
                conn: Connection::open_in_memory()?,
            })),
        };
        archive.initialize_tables()?;

        Ok(archive)
    }

    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let conn = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_WRITE)?;

        Ok(Self {
            inner: Arc::new(Mutex::new(Inner {
                // TODO: Handle non .cyan files in other place.
                path: Some(path.with_extension("cyan")),
                conn,
            })),
        })
    }

    pub fn path(&self) -> Option<PathBuf> {
        self.inner.lock().path.clone()
    }

    pub fn set_path(&mut self, path: PathBuf) {
        self.inner.lock().path = Some(path);
    }

    pub(crate) fn conn(&'_ self) -> MappedMutexGuard<'_, RawMutex, Connection> {
        let lock = self.inner.lock();
        MutexGuard::map(lock, |g| &mut g.conn)
    }

    fn initialize_tables(&self) -> Result<()> {
        let conn = self.conn();
        image_props::initialize_table(&conn)?;
        layer_tree::initialize_table(&conn)?;
        tile_data::initialize_table(&conn)?;
        Ok(())
    }
}
