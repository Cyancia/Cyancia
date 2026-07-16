mod cursor;
mod header;
mod sample;

use anyhow::{Result, bail};
use cursor::Cursor;

pub use header::AbrHeader;
pub use sample::Sample;

pub struct Abr {
    pub header: AbrHeader,
    pub samples: Vec<Sample>,
}

impl Abr {
    pub fn parse(input: &[u8]) -> Result<Self> {
        let mut cursor = Cursor::new(input);
        let header = AbrHeader::parse(&mut cursor)?;
        let mut samples = Vec::new();

        while cursor.remaining() != 0 {
            let signature = cursor.take(4)?;
            if signature != b"8BIM" {
                bail!("unsupported ABR section signature {signature:?}");
            }

            let key = cursor.take(4)?;
            let len = usize::try_from(cursor.read_u32_be()?)?;
            let mut section = cursor.take_cursor(len)?;

            if key == b"samp" {
                samples.extend(Sample::parse_samp_section(&mut section)?);
            }

            if cursor.remaining() != 0 {
                cursor.align_to(4)?;
            }
        }

        Ok(Self { header, samples })
    }
}
