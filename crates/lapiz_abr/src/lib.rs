mod cursor;
mod descriptor;
mod header;
mod hierarchy;
mod pattern;
mod rle;
mod sample;

use anyhow::{Result, bail};
use descriptor::BrushDescriptorRoot;

pub use cursor::Cursor;
pub use descriptor::{
    AbrClass, AbrEnum, AbrIntegerEnum, AbrObject, AbrValue, BlendMode, BrushGroup, BrushPreset,
    BrushTip, ComputedBrushTip, DBrushTip, DescriptorUnit, DualBrush, DynamicsControl,
    EraserToolOptions, PaintToolOptions, PatternReference, PropertyDynamics, RgbColor,
    SampledBrushTip, ShToolOptions, SmudgeToolOptions, ToolOptions, UnitFloat,
};
pub use header::AbrHeader;
pub use hierarchy::HierarchyNode;
pub use lapiz_abr_derive::{AbrClass, AbrEnum, AbrIntegerEnum, AbrObject};
pub use pattern::{ColorMode, Pattern, PatternChannel};
pub use sample::{Sample, SampleImage};

pub struct Abr {
    pub header: AbrHeader,
    pub brushes: Vec<BrushPreset>,
    pub hierarchy: HierarchyNode,
    pub samples: Vec<Sample>,
    pub patterns: Vec<Pattern>,
}

impl Abr {
    pub fn parse(input: &[u8]) -> Result<Self> {
        let mut cursor = Cursor::new(input);
        let header = AbrHeader::parse(&mut cursor)?;
        let mut brushes = Vec::new();
        let mut hierarchy_entries = Vec::new();
        let mut samples = Vec::new();
        let mut patterns = Vec::new();

        while cursor.remaining() != 0 {
            let signature = cursor.take(4)?;
            if signature != b"8BIM" {
                bail!("unsupported ABR section signature {signature:?}");
            }

            let key = cursor.take(4)?;
            let len = usize::try_from(cursor.read_u32_be()?)?;
            let mut section = cursor.take_cursor(len)?;

            if key == b"desc" {
                brushes.extend(BrushDescriptorRoot::parse_desc_section(&mut section)?);
            } else if key == b"phry" {
                hierarchy_entries.extend(hierarchy::parse_phry_section(&mut section)?);
            } else if key == b"samp" {
                samples.extend(Sample::parse_samp_section(&mut section)?);
            } else if key == b"patt" {
                patterns.extend(Pattern::parse_patt_section(&mut section)?);
            }

            if cursor.remaining() != 0 {
                cursor.align_to(4)?;
            }
        }

        let hierarchy = HierarchyNode::from_entries(hierarchy_entries, brushes.len())?;

        Ok(Self {
            header,
            brushes,
            hierarchy,
            samples,
            patterns,
        })
    }
}

#[doc(hidden)]
pub mod __private {
    pub use anyhow::{Error, Result};
}
