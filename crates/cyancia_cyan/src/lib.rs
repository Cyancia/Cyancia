use std::{fs::File, path::Path, sync::Arc};

use anyhow::Result;
use parking_lot::Mutex;
use rusqlite::{Connection, OpenFlags};

pub mod image_props;
pub mod tile_data;
pub use image_props::ImageProperties;

pub struct CyanArchive {
    conn: Arc<Mutex<Connection>>,
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
            conn: Arc::new(Mutex::new(conn)),
        };
        archive.initialize_tables()?;

        Ok(archive)
    }

    pub fn new_in_memory() -> Result<Self> {
        let archive = Self {
            conn: Arc::new(Mutex::new(Connection::open_in_memory()?)),
        };
        archive.initialize_tables()?;

        Ok(archive)
    }

    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let conn = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_WRITE)?;

        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    fn initialize_tables(&self) -> Result<()> {
        let conn = self.conn.lock();
        image_props::initialize_table(&conn)?;
        tile_data::initialize_table(&conn)?;
        Ok(())
    }
}
