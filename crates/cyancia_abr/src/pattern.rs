use anyhow::{Context, Result, bail};
use uuid::Uuid;

use crate::cursor::Cursor;

pub struct Pattern {
    pub id: Uuid,
    pub name: String,
    pub color_mode: u32,
    pub height: u16,
    pub width: u16,
    pub indexed_color_table: Option<Vec<u8>>,
    pub top: u32,
    pub left: u32,
    pub bottom: u32,
    pub right: u32,
    pub channel_count: u32,
    pub channels: Vec<Option<PatternChannel>>,
}

pub struct PatternChannel {
    pub depth: u32,
    pub top: u32,
    pub left: u32,
    pub bottom: u32,
    pub right: u32,
    pub compression: u8,
    pub pixel_data: Vec<u8>,
}

impl Pattern {
    pub(crate) fn parse_patt_section(cursor: &mut Cursor<'_>) -> Result<Vec<Self>> {
        let mut patterns = Vec::new();

        while cursor.remaining() != 0 {
            let len = usize::try_from(cursor.read_u32_be()?)?;
            let mut record = cursor.take_cursor(len)?;

            let version = record.read_u32_be()?;
            if version != 1 {
                bail!("unsupported ABR pattern version {version}");
            }

            let color_mode = record.read_u32_be()?;
            let height = record.read_u16_be()?;
            let width = record.read_u16_be()?;

            let name = record.read_utf16_string()?;

            let id_len = usize::from(record.read_u8()?);
            let id = Uuid::try_parse_ascii(record.take(id_len)?)?;
            let indexed_color_table = if color_mode == 2 {
                Some(record.take(772)?.to_vec())
            } else {
                None
            };

            let array_version = record.read_u32_be()?;
            if array_version != 3 {
                bail!("unsupported ABR pattern array version {array_version}");
            }

            let array_len = usize::try_from(record.read_u32_be()?)?;
            let mut array = record.take_cursor(array_len)?;
            let top = array.read_u32_be()?;
            let left = array.read_u32_be()?;
            let bottom = array.read_u32_be()?;
            let right = array.read_u32_be()?;
            let channel_count = array.read_u32_be()?;
            let slot_count = usize::try_from(
                channel_count
                    .checked_add(2)
                    .context("ABR pattern channel count overflow")?,
            )?;
            let mut channels = Vec::with_capacity(slot_count);

            for _ in 0..slot_count {
                if array.read_u32_be()? == 0 {
                    channels.push(None);
                    continue;
                }

                let channel_len = usize::try_from(array.read_u32_be()?)?;
                if channel_len == 0 {
                    channels.push(None);
                    continue;
                }

                let mut channel = array.take_cursor(channel_len)?;
                let depth = channel.read_u32_be()?;
                let channel_top = channel.read_u32_be()?;
                let channel_left = channel.read_u32_be()?;
                let channel_bottom = channel.read_u32_be()?;
                let channel_right = channel.read_u32_be()?;
                let second_depth = channel.read_u16_be()?;
                if u32::from(second_depth) != depth {
                    bail!("inconsistent ABR pattern channel depths {depth} and {second_depth}");
                }
                let compression = channel.read_u8()?;
                let pixel_data = channel.take(channel.remaining())?.to_vec();

                channels.push(Some(PatternChannel {
                    depth,
                    top: channel_top,
                    left: channel_left,
                    bottom: channel_bottom,
                    right: channel_right,
                    compression,
                    pixel_data,
                }));
            }

            if array.remaining() != 0 || record.remaining() != 0 {
                bail!("unexpected trailing ABR pattern data");
            }

            patterns.push(Self {
                id,
                name,
                color_mode,
                height,
                width,
                indexed_color_table,
                top,
                left,
                bottom,
                right,
                channel_count,
                channels,
            });

            cursor.align_to(4)?;
        }

        Ok(patterns)
    }
}
