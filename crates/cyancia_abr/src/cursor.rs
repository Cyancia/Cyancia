use anyhow::{Context, Result, bail};

pub(crate) struct Cursor<'a> {
    input: &'a [u8],
    position: usize,
}

impl<'a> Cursor<'a> {
    pub(crate) fn new(input: &'a [u8]) -> Self {
        Self { input, position: 0 }
    }

    pub(crate) fn remaining(&self) -> usize {
        self.input.len() - self.position
    }

    pub(crate) fn read_u8(&mut self) -> Result<u8> {
        Ok(self.take(1)?[0])
    }

    pub(crate) fn read_u16_be(&mut self) -> Result<u16> {
        let bytes = self.take(2)?;

        Ok(u16::from_be_bytes([bytes[0], bytes[1]]))
    }

    pub(crate) fn read_i32_be(&mut self) -> Result<i32> {
        let bytes = self.take(4)?;

        Ok(i32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }

    pub(crate) fn read_u32_be(&mut self) -> Result<u32> {
        let bytes = self.take(4)?;

        Ok(u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }

    pub(crate) fn read_utf16_string(&mut self) -> Result<String> {
        let len = usize::try_from(self.read_u32_be()?)?;
        let bytes = self.take(
            len.checked_mul(2)
                .context("ABR UTF-16 string length overflow")?,
        )?;
        let mut units = bytes
            .chunks_exact(2)
            .map(|bytes| u16::from_be_bytes([bytes[0], bytes[1]]))
            .collect::<Vec<_>>();
        if units.last() == Some(&0) {
            units.pop();
        }

        Ok(String::from_utf16(&units)?)
    }

    pub(crate) fn take(&mut self, len: usize) -> Result<&'a [u8]> {
        let end = self
            .position
            .checked_add(len)
            .context("ABR data length overflow")?;
        let bytes = self
            .input
            .get(self.position..end)
            .context("unexpected end of ABR data")?;
        self.position = end;

        Ok(bytes)
    }

    pub(crate) fn take_cursor(&mut self, len: usize) -> Result<Self> {
        Ok(Self::new(self.take(len)?))
    }

    pub(crate) fn skip(&mut self, len: usize) -> Result<()> {
        self.take(len)?;
        Ok(())
    }

    pub(crate) fn align_to(&mut self, n: usize) -> Result<()> {
        if n == 0 {
            bail!("ABR alignment must be greater than zero");
        }

        let remainder = self.position % n;
        if remainder != 0 {
            self.skip(n - remainder)?;
        }

        Ok(())
    }
}
