use std::{
    collections::{HashMap, HashSet, VecDeque},
    io::{Cursor, Read, Write},
};

use anyhow::anyhow;
use lapiz_assets::{asset::Asset, loader::AssetSerializer};
use lapiz_shader_graph::save::{SerializableExternalVariable, SerializableGraph};
use lapiz_utils::wrapper;
use serde::{Deserialize, Serialize, Serializer, de::Deserializer};
use uuid::Uuid;
use zip::{ZipArchive, ZipWriter, write::FileOptions};

/// A serializable filter preset: an ordered list of shader groups forming a
/// directed acyclic chain plus a shared set of external variables.
pub struct FilterPreset {
    pub metadata: FilterPresetMetadata,
    /// Ordered; serves as the execution reference order.
    pub groups: Vec<SerializableFilterGroup>,
    pub external_vars: Vec<SerializableExternalVariable>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct FilterPresetMetadata {
    pub name: String,
}

/// One shader group within a filter preset. The graph body is not part of the
/// header; it is stored as a separate `<group_id>.csg` file in the .lfp zip.
pub struct SerializableFilterGroup {
    pub id: FilterGroupId,
    pub name: String,
    pub input: FilterSlotRef,
    pub output: FilterSlotRef,
    pub graph: SerializableGraph,
}

wrapper! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
    pub FilterGroupId : Uuid
}

impl FilterGroupId {
    pub fn random() -> Self {
        Self::new(Uuid::new_v4())
    }
}

impl std::fmt::Display for FilterGroupId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Where a shader group reads from / writes to. Serde representation is the
/// string `"layer"` or `"group:<uuid>"`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FilterSlotRef {
    Layer,
    Group(Uuid),
}

impl Serialize for FilterSlotRef {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            FilterSlotRef::Layer => serializer.serialize_str("layer"),
            FilterSlotRef::Group(id) => serializer.serialize_str(&format!("group:{id}")),
        }
    }
}

impl<'de> Deserialize<'de> for FilterSlotRef {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        if s == "layer" {
            Ok(FilterSlotRef::Layer)
        } else if let Some(rest) = s.strip_prefix("group:") {
            Uuid::parse_str(rest)
                .map(FilterSlotRef::Group)
                .map_err(serde::de::Error::custom)
        } else {
            Err(serde::de::Error::custom(format!(
                "Invalid filter slot reference: {s}"
            )))
        }
    }
}

impl Asset for FilterPreset {
    const TYPE_NAME: &'static str = "filter_preset";
}

/// Header document stored in `filter.toml` inside the .lfp zip.
#[derive(Serialize, Deserialize)]
struct FilterToml {
    name: String,
    groups: Vec<FilterTomlGroup>,
}

#[derive(Serialize, Deserialize)]
struct FilterTomlGroup {
    id: FilterGroupId,
    name: String,
    input: FilterSlotRef,
    output: FilterSlotRef,
}

#[derive(Default)]
pub struct FilterPresetSerializer;

#[derive(Debug, thiserror::Error)]
pub enum FilterPresetSerializerError {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Zip(#[from] zip::result::ZipError),
    #[error(transparent)]
    TomlDe(#[from] toml::de::Error),
    #[error(transparent)]
    TomlSer(#[from] toml::ser::Error),
    #[error("Invalid filter preset: {0}")]
    Invalid(String),
}

impl AssetSerializer for FilterPresetSerializer {
    type Asset = FilterPreset;

    type Error = FilterPresetSerializerError;

    fn file_extension() -> &'static str {
        "lfp"
    }

    fn read(&self, reader: &mut dyn Read) -> Result<Self::Asset, Self::Error> {
        let mut buf = Vec::new();
        reader.read_to_end(&mut buf)?;
        let mut archive = ZipArchive::new(Cursor::new(buf))?;

        let mut filter_buffer = String::new();
        archive
            .by_name("filter.toml")?
            .read_to_string(&mut filter_buffer)?;
        let filter_toml = toml::from_str::<FilterToml>(&filter_buffer)?;

        let external_vars = match archive.by_name("external_vars.toml") {
            Ok(mut f) => {
                let mut external_vars_buffer = String::new();
                f.read_to_string(&mut external_vars_buffer)?;
                external_vars_buffer
                    .parse::<toml::Value>()?
                    .try_into::<Vec<SerializableExternalVariable>>()?
            }
            Err(_) => Default::default(),
        };

        // Load every `<group_id>.csg` and index it by group id.
        let files = archive
            .file_names()
            .map(ToString::to_string)
            .collect::<Vec<_>>();
        let mut graphs: HashMap<FilterGroupId, SerializableGraph> = HashMap::new();
        for file in files {
            if let Some(stem) = file.strip_suffix(".csg") {
                let id = FilterGroupId::new(Uuid::parse_str(stem).map_err(|e| {
                    FilterPresetSerializerError::Invalid(format!(
                        "Invalid group id in csg filename {file}: {e}"
                    ))
                })?);
                let mut graph_buffer = String::new();
                archive.by_name(&file)?.read_to_string(&mut graph_buffer)?;
                let graph = toml::from_str::<SerializableGraph>(&graph_buffer)?;
                graphs.insert(id, graph);
            }
        }

        let mut groups = Vec::with_capacity(filter_toml.groups.len());
        for header in filter_toml.groups {
            let graph = graphs.remove(&header.id).ok_or_else(|| {
                FilterPresetSerializerError::Invalid(format!(
                    "Missing graph file for group {}",
                    header.id.0
                ))
            })?;
            groups.push(SerializableFilterGroup {
                id: header.id,
                name: header.name,
                input: header.input,
                output: header.output,
                graph,
            });
        }

        let preset = FilterPreset {
            metadata: FilterPresetMetadata {
                name: filter_toml.name,
            },
            groups,
            external_vars,
        };

        validate_preset(&preset)
            .map_err(|e| FilterPresetSerializerError::Invalid(e.to_string()))?;

        Ok(preset)
    }

    fn write(
        &self,
        asset: &Self::Asset,
        writer: &mut dyn std::io::Write,
    ) -> Result<(), Self::Error> {
        validate_preset(asset).map_err(|e| FilterPresetSerializerError::Invalid(e.to_string()))?;

        let mut buf = Vec::new();
        let mut zip = ZipWriter::new(Cursor::new(&mut buf));

        let toml_doc = FilterToml {
            name: asset.metadata.name.clone(),
            groups: asset
                .groups
                .iter()
                .map(|g| FilterTomlGroup {
                    id: g.id,
                    name: g.name.clone(),
                    input: g.input,
                    output: g.output,
                })
                .collect(),
        };

        zip.start_file("filter.toml", FileOptions::<()>::default())?;
        let toml_buffer = toml::to_string(&toml_doc)?;
        zip.write_all(toml_buffer.as_bytes())?;

        if !asset.external_vars.is_empty() {
            zip.start_file("external_vars.toml", FileOptions::<()>::default())?;
            let external_vars_buffer = toml::Value::try_from(&asset.external_vars)?.to_string();
            zip.write_all(external_vars_buffer.as_bytes())?;
        }

        for group in &asset.groups {
            zip.start_file(format!("{}.csg", group.id.0), FileOptions::<()>::default())?;
            let graph_buffer = toml::to_string(&group.graph)?;
            zip.write_all(graph_buffer.as_bytes())?;
        }

        zip.finish()?;
        writer.write_all(&buf)?;

        Ok(())
    }
}

/// Validates a filter preset against the structural rules in the design doc:
/// 1. At least one group; group ids are unique.
/// 2. Every `Group(x)` input/output reference exists and is not self.
/// 3. Exactly one group outputs to Layer.
/// 4. If group A outputs to Group(B), then B's input must be Group(A).
/// 5. The group connection graph is acyclic (Kahn topological sort).
pub fn validate_preset(preset: &FilterPreset) -> anyhow::Result<()> {
    if preset.groups.is_empty() {
        return Err(anyhow!("Filter preset must contain at least one group"));
    }

    // Rule 1: unique ids.
    let mut seen = HashSet::new();
    for group in &preset.groups {
        if !seen.insert(group.id) {
            return Err(anyhow!("Duplicate group id: {}", group.id.0));
        }
    }
    let ids = seen;

    // Rule 3: exactly one group outputs to Layer.
    let layer_outputs = preset
        .groups
        .iter()
        .filter(|g| g.output == FilterSlotRef::Layer)
        .count();
    if layer_outputs != 1 {
        return Err(anyhow!(
            "Exactly one group must output to Layer; found {layer_outputs}"
        ));
    }

    // Rule 2: input/output Group references exist and are not self.
    for group in &preset.groups {
        if let FilterSlotRef::Group(target) = group.input {
            let target = FilterGroupId::new(target);
            if !ids.contains(&target) {
                return Err(anyhow!(
                    "Group {} references nonexistent input group {}",
                    group.id.0,
                    target.0
                ));
            }
            if target == group.id {
                return Err(anyhow!(
                    "Group {} references itself as its input",
                    group.id.0
                ));
            }
        }
        if let FilterSlotRef::Group(target) = group.output {
            let target = FilterGroupId::new(target);
            if !ids.contains(&target) {
                return Err(anyhow!(
                    "Group {} references nonexistent output group {}",
                    group.id.0,
                    target.0
                ));
            }
            if target == group.id {
                return Err(anyhow!(
                    "Group {} references itself as its output",
                    group.id.0
                ));
            }
        }
    }

    // Rule 4: if A.output == Group(B), then B.input == Group(A).
    for group in &preset.groups {
        if let FilterSlotRef::Group(target) = group.output {
            let consumer = preset
                .groups
                .iter()
                .find(|c| c.id.0 == target)
                .expect("referenced group exists (checked above)");
            if consumer.input != FilterSlotRef::Group(group.id.0) {
                return Err(anyhow!(
                    "Group {} outputs to {} but that group's input is not Group({})",
                    group.id.0,
                    target,
                    group.id.0
                ));
            }
        }
    }

    // Rule 5: acyclicity via Kahn topological sort.
    let mut indegree: HashMap<FilterGroupId, usize> =
        preset.groups.iter().map(|g| (g.id, 0)).collect();
    let mut edges: HashMap<FilterGroupId, Vec<FilterGroupId>> = HashMap::new();
    for group in &preset.groups {
        if let FilterSlotRef::Group(target) = group.output {
            let target = FilterGroupId::new(target);
            edges.entry(group.id).or_default().push(target);
            *indegree.entry(target).or_insert(0) += 1;
        }
    }
    let mut queue: VecDeque<FilterGroupId> = indegree
        .iter()
        .filter(|(_, d)| **d == 0)
        .map(|(id, _)| *id)
        .collect();
    let mut order = Vec::with_capacity(preset.groups.len());
    while let Some(id) = queue.pop_front() {
        order.push(id);
        if let Some(nexts) = edges.get(&id) {
            for next in nexts {
                let d = indegree.get_mut(next).expect("target in indegree map");
                *d -= 1;
                if *d == 0 {
                    queue.push_back(*next);
                }
            }
        }
    }
    if order.len() != preset.groups.len() {
        return Err(anyhow!("Filter group graph contains a cycle"));
    }

    Ok(())
}
