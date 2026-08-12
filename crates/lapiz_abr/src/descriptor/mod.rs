use anyhow::{Context, Result, bail};
use uuid::Uuid;

use crate::Cursor;

mod class;

pub use class::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DescriptorUnit {
    Angle,
    Percent,
    Pixels,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct UnitFloat {
    pub unit: DescriptorUnit,
    pub value: f64,
}

pub trait AbrValue: Sized {
    fn parse_value(cursor: &mut Cursor<'_>, value_type: [u8; 4], offset: usize) -> Result<Self>;

    fn expect_type(actual: [u8; 4], expected: [u8; 4], offset: usize) -> Result<()> {
        if actual != expected {
            bail!(
                "expected ABR descriptor value type {expected:?}, found {actual:?} at desc offset {offset}"
            );
        }
        Ok(())
    }
}

pub trait AbrObject: Sized {
    fn parse_with_header(
        cursor: &mut Cursor<'_>,
        class_id: String,
        entry_count: usize,
        header_offset: usize,
    ) -> Result<Self>;

    fn parse(cursor: &mut Cursor<'_>) -> Result<Self> {
        let (class_id, entry_count, header_offset) = Self::read_header(cursor)?;
        Self::parse_with_header(cursor, class_id, entry_count, header_offset)
    }

    fn read_header(cursor: &mut Cursor<'_>) -> Result<(String, usize, usize)> {
        cursor.read_utf16_string()?;
        let offset = cursor.position();
        let class_id = cursor.read_descriptor_id()?;
        let entry_count = usize::try_from(cursor.read_u32_be()?)?;
        Ok((class_id, entry_count, offset))
    }

    fn skip_value(cursor: &mut Cursor<'_>, value_type: [u8; 4], offset: usize) -> Result<()> {
        match &value_type {
            b"Objc" => {
                cursor.read_utf16_string()?;
                cursor.read_descriptor_id()?;
                let count = usize::try_from(cursor.read_u32_be()?)?;
                for _ in 0..count {
                    cursor.read_descriptor_id()?;
                    let value_offset = cursor.position();
                    let value_type = cursor.read_ostype()?;
                    Self::skip_value(cursor, value_type, value_offset)?;
                }
            }
            b"VlLs" => {
                let count = usize::try_from(cursor.read_u32_be()?)?;
                for _ in 0..count {
                    let value_offset = cursor.position();
                    let value_type = cursor.read_ostype()?;
                    Self::skip_value(cursor, value_type, value_offset)?;
                }
            }
            b"TEXT" => {
                cursor.read_utf16_string()?;
            }
            b"enum" => {
                cursor.read_descriptor_id()?;
                cursor.read_descriptor_id()?;
            }
            b"UntF" => {
                let unit_offset = cursor.position();
                match cursor.take(4)? {
                    b"#Ang" | b"#Prc" | b"#Pxl" => {}
                    unit => {
                        bail!(
                            "unsupported ABR descriptor unit {unit:?} at desc offset {unit_offset}"
                        );
                    }
                }
                cursor.skip(8)?;
            }
            b"doub" => cursor.skip(8)?,
            b"long" => cursor.skip(4)?,
            b"bool" => cursor.skip(1)?,
            _ => {
                bail!(
                    "unsupported ABR descriptor value type {value_type:?} at desc offset {offset}"
                );
            }
        }
        Ok(())
    }
}

impl<T: AbrObject> AbrValue for T {
    fn parse_value(cursor: &mut Cursor<'_>, value_type: [u8; 4], offset: usize) -> Result<Self> {
        Self::expect_type(value_type, *b"Objc", offset)?;
        let (class_id, entry_count, header_offset) = T::read_header(cursor)?;
        T::parse_with_header(cursor, class_id, entry_count, header_offset)
    }
}

pub trait AbrClass: AbrObject {
    const CLASS_ID: &'static str;
}

pub trait AbrEnum: AbrValue {
    const TYPE_ID: &'static str;

    fn from_value_id(value_id: &str) -> Option<Self>;

    fn parse_enum_value(
        cursor: &mut Cursor<'_>,
        value_type: [u8; 4],
        offset: usize,
    ) -> Result<Self> {
        Self::expect_type(value_type, *b"enum", offset)?;
        let type_id = cursor.read_descriptor_id()?;
        if type_id != Self::TYPE_ID {
            bail!(
                "expected ABR descriptor enum type {:?}, found {:?} at desc offset {offset}",
                Self::TYPE_ID,
                type_id,
            );
        }
        let value_id = cursor.read_descriptor_id()?;
        Self::from_value_id(&value_id).ok_or_else(|| {
            anyhow::anyhow!(
                "unsupported ABR descriptor enum {:?}.{:?} at desc offset {offset}",
                Self::TYPE_ID,
                value_id,
            )
        })
    }
}

pub trait AbrIntegerEnum: AbrValue {
    fn from_i32(value: i32) -> Option<Self>;

    fn parse_integer_value(
        cursor: &mut Cursor<'_>,
        value_type: [u8; 4],
        offset: usize,
    ) -> Result<Self> {
        Self::expect_type(value_type, *b"long", offset)?;
        let value = cursor.read_i32_be()?;
        Self::from_i32(value).ok_or_else(|| {
            anyhow::anyhow!(
                "unsupported ABR descriptor integer enum {} value {value} at desc offset {offset}",
                std::any::type_name::<Self>(),
            )
        })
    }
}

impl AbrValue for String {
    fn parse_value(cursor: &mut Cursor<'_>, value_type: [u8; 4], offset: usize) -> Result<Self> {
        Self::expect_type(value_type, *b"TEXT", offset)?;
        cursor.read_utf16_string()
    }
}

impl AbrValue for Uuid {
    fn parse_value(cursor: &mut Cursor<'_>, value_type: [u8; 4], offset: usize) -> Result<Self> {
        Self::expect_type(value_type, *b"TEXT", offset)?;
        let value = cursor.read_utf16_string()?;
        Uuid::parse_str(&value)
            .with_context(|| format!("invalid ABR descriptor UUID at desc offset {offset}"))
    }
}

impl AbrValue for bool {
    fn parse_value(cursor: &mut Cursor<'_>, value_type: [u8; 4], offset: usize) -> Result<Self> {
        Self::expect_type(value_type, *b"bool", offset)?;
        Ok(cursor.read_u8()? != 0)
    }
}

impl AbrValue for i32 {
    fn parse_value(cursor: &mut Cursor<'_>, value_type: [u8; 4], offset: usize) -> Result<Self> {
        Self::expect_type(value_type, *b"long", offset)?;
        cursor.read_i32_be()
    }
}

impl AbrValue for f64 {
    fn parse_value(cursor: &mut Cursor<'_>, value_type: [u8; 4], offset: usize) -> Result<Self> {
        Self::expect_type(value_type, *b"doub", offset)?;
        cursor.read_f64_be()
    }
}

impl AbrValue for UnitFloat {
    fn parse_value(cursor: &mut Cursor<'_>, value_type: [u8; 4], offset: usize) -> Result<Self> {
        Self::expect_type(value_type, *b"UntF", offset)?;
        let unit_offset = cursor.position();
        let unit = match cursor.take(4)? {
            b"#Ang" => DescriptorUnit::Angle,
            b"#Prc" => DescriptorUnit::Percent,
            b"#Pxl" => DescriptorUnit::Pixels,
            unit => {
                bail!("unsupported ABR descriptor unit {unit:?} at desc offset {unit_offset}");
            }
        };
        Ok(Self {
            unit,
            value: cursor.read_f64_be()?,
        })
    }
}

impl<T: AbrValue> AbrValue for Vec<T> {
    fn parse_value(cursor: &mut Cursor<'_>, value_type: [u8; 4], offset: usize) -> Result<Self> {
        Self::expect_type(value_type, *b"VlLs", offset)?;
        let count = usize::try_from(cursor.read_u32_be()?)?;
        let mut values = Vec::with_capacity(count);
        for _ in 0..count {
            let value_offset = cursor.position();
            let value_type = cursor.read_ostype()?;
            values.push(T::parse_value(cursor, value_type, value_offset)?);
        }
        Ok(values)
    }
}
