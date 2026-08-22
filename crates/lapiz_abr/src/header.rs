use anyhow::{Result, bail};

use crate::cursor::Cursor;

#[derive(Debug)]
pub struct AbrHeader {
    pub major: u16,
    pub subversion: u16,
}

impl AbrHeader {
    pub(crate) fn parse(cursor: &mut Cursor<'_>) -> Result<Self> {
        let major = cursor.read_u16_be()?;
        let subversion = cursor.read_u16_be()?;

        if major < 6 {
            bail!("unsupported ABR major version {major}");
        }
        if subversion != 2 {
            bail!("unsupported ABR subversion {subversion}");
        }

        Ok(Self { major, subversion })
    }
}
