use anyhow::{Result, anyhow, bail};
use rusqlite::{Connection, params};
use uuid::Uuid;

use crate::CyanArchive;

#[derive(Debug, Clone)]
pub struct ImageProperties {
    pub width: u32,
    pub height: u32,
    pub tile_size: u32,
    pub color_profile: Vec<u8>,
    pub root_layer: Uuid,
}

pub(crate) fn initialize_table(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        r#"
CREATE TABLE image (
    width         INTEGER NOT NULL,
    height        INTEGER NOT NULL,
    tile_size     INTEGER NOT NULL,
    color_profile BLOB NOT NULL,
    root_layer    BLOB NOT NULL CHECK (length(root_layer) = 16)
);

CREATE UNIQUE INDEX image_singleton ON image ((1));
        "#,
    )?;

    Ok(())
}

impl CyanArchive {
    pub fn read_image_properties(&self) -> Result<ImageProperties> {
        let conn = self.conn.lock();

        let mut statement = conn
            .prepare("SELECT width, height, tile_size, color_profile, root_layer FROM image")?;
        let mut rows = statement.query([])?;
        let row = rows
            .next()?
            .ok_or_else(|| anyhow!("archive does not contain image properties"))?;

        let width = u32::try_from(row.get::<_, i64>(0)?)?;
        let height = u32::try_from(row.get::<_, i64>(1)?)?;
        let tile_size = u32::try_from(row.get::<_, i64>(2)?)?;
        let color_profile = row.get(3)?;
        let root_layer = Uuid::from_slice(&row.get::<_, Vec<u8>>(4)?)?;

        if rows.next()?.is_some() {
            bail!("archive contains more than one image properties row");
        }

        Ok(ImageProperties {
            width,
            height,
            tile_size,
            color_profile,
            root_layer,
        })
    }

    pub fn write_image_properties(&mut self, properties: &ImageProperties) -> Result<()> {
        let mut conn = self.conn.lock();

        let transaction = conn.transaction()?;
        transaction.execute("DELETE FROM image", [])?;
        transaction.execute(
            r#"
INSERT INTO image (width, height, tile_size, color_profile, root_layer)
VALUES (?1, ?2, ?3, ?4, ?5)
            "#,
            params![
                properties.width,
                properties.height,
                properties.tile_size,
                properties.color_profile,
                &properties.root_layer.as_bytes()[..],
            ],
        )?;
        transaction.commit()?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn image_table_has_exactly_the_requested_columns() {
        let archive = CyanArchive::new_in_memory().unwrap();
        let conn = archive.conn.lock();
        let mut statement = conn.prepare("PRAGMA table_info(image)").unwrap();
        let columns = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, bool>(3)?,
                ))
            })
            .unwrap()
            .collect::<rusqlite::Result<Vec<_>>>()
            .unwrap();

        assert_eq!(
            columns,
            [
                ("width".into(), "INTEGER".into(), true),
                ("height".into(), "INTEGER".into(), true),
                ("tile_size".into(), "INTEGER".into(), true),
                ("color_profile".into(), "BLOB".into(), true),
                ("root_layer".into(), "BLOB".into(), true),
            ]
        );
    }

    #[test]
    fn image_properties_round_trip_with_embedded_icc_profile() {
        let mut archive = CyanArchive::new_in_memory().unwrap();
        let properties = ImageProperties {
            width: 1920,
            height: 1080,
            tile_size: 256,
            color_profile: vec![0, 1, 2, 3],
            root_layer: Uuid::new_v4(),
        };

        archive.write_image_properties(&properties).unwrap();

        let stored_profile: Vec<u8> = archive
            .conn
            .lock()
            .query_row("SELECT color_profile FROM image", [], |row| row.get(0))
            .unwrap();
        assert_eq!(stored_profile, properties.color_profile);

        let restored = archive.read_image_properties().unwrap();
        assert_eq!(restored.width, properties.width);
        assert_eq!(restored.height, properties.height);
        assert_eq!(restored.tile_size, properties.tile_size);
        assert_eq!(restored.color_profile, properties.color_profile);
        assert_eq!(restored.root_layer, properties.root_layer);
    }

    #[test]
    fn writing_image_properties_replaces_the_single_row() {
        let mut archive = CyanArchive::new_in_memory().unwrap();
        for width in [128, 512] {
            archive
                .write_image_properties(&ImageProperties {
                    width,
                    height: 256,
                    tile_size: 256,
                    color_profile: vec![0, 1, 2, 3],
                    root_layer: Uuid::new_v4(),
                })
                .unwrap();
        }

        let row_count: u32 = archive
            .conn
            .lock()
            .query_row("SELECT COUNT(*) FROM image", [], |row| row.get(0))
            .unwrap();
        assert_eq!(row_count, 1);
        assert_eq!(archive.read_image_properties().unwrap().width, 512);
    }

    #[test]
    fn arbitrary_color_profile_bytes_are_accepted() {
        let archive = CyanArchive::new_in_memory().unwrap();
        let root_layer = Uuid::new_v4();
        archive
            .conn
            .lock()
            .execute(
                r#"
INSERT INTO image (width, height, tile_size, color_profile, root_layer)
VALUES (1, 1, 256, X'00010203', ?1)
                "#,
                params![&root_layer.as_bytes()[..]],
            )
            .unwrap();

        assert_eq!(
            archive.read_image_properties().unwrap().color_profile,
            [0, 1, 2, 3]
        );
    }
}
