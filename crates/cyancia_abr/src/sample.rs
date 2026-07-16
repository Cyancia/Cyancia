use anyhow::Result;
use uuid::Uuid;

use crate::cursor::Cursor;

pub struct Sample {
    pub id: Uuid,
    pub top: i32,
    pub left: i32,
    pub bottom: i32,
    pub right: i32,
    pub depth: u16,
    pub compression: u8,
    pub pixel_data: Vec<u8>,
}

impl Sample {
    pub(crate) fn parse_samp_section(cursor: &mut Cursor<'_>) -> Result<Vec<Self>> {
        let mut samples = Vec::new();

        while cursor.remaining() != 0 {
            let len = usize::try_from(cursor.read_u32_be()?)?;
            let mut record = cursor.take_cursor(len)?;
            let id_len = usize::from(record.read_u8()?);
            let id = Uuid::try_parse_ascii(record.take(id_len)?)?;

            // unknown
            record.skip(264)?;

            let top = record.read_i32_be()?;
            let left = record.read_i32_be()?;
            let bottom = record.read_i32_be()?;
            let right = record.read_i32_be()?;
            let depth = record.read_u16_be()?;
            let compression = record.read_u8()?;
            let pixel_data = record.take(record.remaining())?.to_vec();

            samples.push(Self {
                id,
                top,
                left,
                bottom,
                right,
                depth,
                compression,
                pixel_data,
            });

            cursor.align_to(4)?;
        }

        Ok(samples)
    }
}
