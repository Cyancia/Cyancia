use anyhow::{Context, Result};

pub(crate) struct Cursor<'a> {
    input: &'a [u8],
    position: usize,
}

impl<'a> Cursor<'a> {
    pub(crate) fn new(input: &'a [u8]) -> Self {
        Self { input, position: 0 }
    }

    pub(crate) fn read_u16_be(&mut self) -> Result<u16> {
        let bytes = self
            .input
            .get(self.position..self.position + 2)
            .context("unexpected end of ABR data")?;
        self.position += 2;

        Ok(u16::from_be_bytes([bytes[0], bytes[1]]))
    }
}
