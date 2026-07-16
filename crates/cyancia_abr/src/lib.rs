mod cursor;
mod header;

use anyhow::Result;
use cursor::Cursor;

pub use header::AbrHeader;

pub struct Abr {
    pub header: AbrHeader,
}

impl Abr {
    pub fn parse(input: &[u8]) -> Result<Self> {
        let mut cursor = Cursor::new(input);

        Ok(Self {
            header: AbrHeader::parse(&mut cursor)?,
        })
    }
}
