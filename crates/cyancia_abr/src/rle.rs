use anyhow::{Context, Result, ensure};

use crate::Cursor;

pub(crate) fn decode(
    cursor: &mut Cursor<'_>,
    row_count: usize,
    row_bytes: usize,
) -> Result<Vec<u8>> {
    let decoded_len = row_count
        .checked_mul(row_bytes)
        .context("RLE image size overflows usize")?;
    let mut row_lengths = Vec::with_capacity(row_count);
    for _ in 0..row_count {
        row_lengths.push(usize::from(cursor.read_u16_be()?));
    }

    let mut decoded = Vec::with_capacity(decoded_len);
    for (row, compressed_len) in row_lengths.into_iter().enumerate() {
        let compressed = cursor
            .take(compressed_len)
            .with_context(|| format!("compressed row {row} exceeds the available data"))?;
        let row_start = decoded.len();
        let mut offset = 0;

        while offset < compressed.len() {
            let control = compressed[offset] as i8;
            offset += 1;

            match control {
                0..=127 => {
                    let count = usize::from(control as u8) + 1;
                    let end = offset
                        .checked_add(count)
                        .context("PackBits literal length overflows usize")?;
                    ensure!(
                        end <= compressed.len(),
                        "truncated PackBits literal in row {row}"
                    );
                    ensure!(
                        decoded.len() - row_start + count <= row_bytes,
                        "PackBits literal exceeds row {row}"
                    );
                    decoded.extend_from_slice(&compressed[offset..end]);
                    offset = end;
                }
                -127..=-1 => {
                    ensure!(
                        offset < compressed.len(),
                        "truncated PackBits repeat in row {row}"
                    );
                    let count = usize::try_from(1 - i16::from(control))?;
                    ensure!(
                        decoded.len() - row_start + count <= row_bytes,
                        "PackBits repeat exceeds row {row}"
                    );
                    decoded.extend(std::iter::repeat_n(compressed[offset], count));
                    offset += 1;
                }
                -128 => {}
            }
        }

        ensure!(
            decoded.len() - row_start == row_bytes,
            "row {row} decoded to {} bytes, expected {row_bytes}",
            decoded.len() - row_start
        );
    }

    Ok(decoded)
}
