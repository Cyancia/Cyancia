use anyhow::{Result, bail};
use cyancia_abr_derive::{AbrClass, AbrObject};
use uuid::Uuid;

use crate::{AbrObject as _, Cursor};

#[derive(Debug)]
pub struct HierarchyNode {
    pub name: String,
    pub id: Option<Uuid>,
    pub brushes: Vec<usize>,
    pub children: Vec<HierarchyNode>,
}

impl HierarchyNode {
    pub(crate) fn from_entries(entries: Vec<HierarchyEntry>, brush_count: usize) -> Result<Self> {
        let mut stack = vec![Self {
            name: String::new(),
            id: None,
            brushes: Vec::new(),
            children: Vec::new(),
        }];
        let mut next_brush = 0;

        for entry in entries {
            match entry {
                HierarchyEntry::Group(group) => stack.push(Self {
                    name: group.name,
                    id: group.id,
                    brushes: Vec::new(),
                    children: Vec::new(),
                }),
                HierarchyEntry::GroupEnd(_) => {
                    if stack.len() == 1 {
                        bail!("unmatched ABR hierarchy groupEnd");
                    }
                    let Some(group) = stack.pop() else {
                        bail!("invalid empty ABR hierarchy stack");
                    };
                    let Some(parent) = stack.last_mut() else {
                        bail!("invalid ABR hierarchy stack without root");
                    };
                    parent.children.push(group);
                }
                HierarchyEntry::Preset(_) => {
                    if next_brush == brush_count {
                        bail!("ABR hierarchy contains more presets than descriptor brushes");
                    }
                    let Some(group) = stack.last_mut() else {
                        bail!("invalid empty ABR hierarchy stack");
                    };
                    group.brushes.push(next_brush);
                    next_brush += 1;
                }
            }
        }

        if stack.len() != 1 {
            bail!("ABR hierarchy contains an unclosed group");
        }

        let Some(mut root) = stack.pop() else {
            bail!("invalid empty ABR hierarchy stack");
        };
        root.brushes.extend(next_brush..brush_count);
        Ok(root)
    }
}

#[derive(AbrClass)]
#[abr(class = "null")]
struct HierarchyDescriptorRoot {
    #[abr(key = "hierarchy")]
    entries: Vec<HierarchyEntry>,
}

#[derive(AbrObject)]
pub(crate) enum HierarchyEntry {
    Group(HierarchyGroup),
    GroupEnd(HierarchyGroupEnd),
    Preset(HierarchyPreset),
}

#[derive(AbrClass)]
#[abr(class = "Grup")]
pub(crate) struct HierarchyGroup {
    #[abr(key = "Nm  ")]
    name: String,
    #[abr(key = "zuid")]
    id: Option<Uuid>,
}

#[derive(AbrClass)]
#[abr(class = "groupEnd")]
pub(crate) struct HierarchyGroupEnd {}

#[derive(AbrClass)]
#[abr(class = "preset")]
pub(crate) struct HierarchyPreset {}

pub(crate) fn parse_phry_section(cursor: &mut Cursor<'_>) -> Result<Vec<HierarchyEntry>> {
    let version = cursor.read_u32_be()?;
    if version != 16 {
        bail!("unsupported ABR hierarchy descriptor version {version}");
    }

    let root = HierarchyDescriptorRoot::parse(cursor)?;
    if cursor.remaining() != 0 {
        bail!("unexpected trailing ABR hierarchy data");
    }
    Ok(root.entries)
}
