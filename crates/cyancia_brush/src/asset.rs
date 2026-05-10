use std::io::{Cursor, Read, Write};

use bevy_math::IRect;
use cyancia_assets::{
    asset::{Asset, AssetHandle, AssetId},
    loader::AssetSerializer,
};
use cyancia_render::texture::ImageSerializerError;
use cyancia_shader_graph::save::{
    GraphDeserializeError, GraphSerializable, SerializableExternalVariable, SerializableGraph,
    SerializableGraphLiteral,
};
use image::{DynamicImage, ImageFormat};
use serde::{Deserialize, Serialize};
use wgpu::{
    Device, Extent3d, Queue, Texture, TextureDimension, TextureFormat, TextureUsages,
    util::DeviceExt,
    wgt::{TextureDataOrder, TextureDescriptor},
};
use zip::{ZipArchive, ZipWriter, write::FileOptions};

pub struct BrushPreset {
    pub metadata: BrushPresetMetadata,
    pub required_spacing_graph: SerializableGraph,
    pub main_graph: SerializableGraph,
    pub stroke_postprocess_graphs: Vec<SerializableGraph>,
    pub external_vars: Vec<SerializableExternalVariable>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct BrushPresetMetadata {
    pub name: String,
}

impl Asset for BrushPreset {
    const TYPE_NAME: &'static str = "brush_preset";
}

#[derive(Default)]
pub struct BrushPresetSerializer;

#[derive(Debug, thiserror::Error)]
pub enum BrushPresetSerializerError {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Zip(#[from] zip::result::ZipError),
    #[error(transparent)]
    TomlDe(#[from] toml::de::Error),
    #[error(transparent)]
    TomlSer(#[from] toml::ser::Error),
    #[error(transparent)]
    Image(#[from] ImageSerializerError),
}

impl AssetSerializer for BrushPresetSerializer {
    type Asset = BrushPreset;

    type Error = BrushPresetSerializerError;

    fn file_extension() -> &'static str {
        "cbp"
    }

    // TODO: Final .cbp file definition.
    // TODO: Support embedded textures and shader graph functions.
    fn read(&self, reader: &mut dyn Read) -> Result<Self::Asset, Self::Error> {
        let mut buf = Vec::new();
        reader.read_to_end(&mut buf)?;
        let mut archive = ZipArchive::new(std::io::Cursor::new(buf))?;

        let mut main_graph_buffer = String::new();
        archive
            .by_name("main.csg")?
            .read_to_string(&mut main_graph_buffer)?;
        let main_graph = toml::from_str::<SerializableGraph>(&main_graph_buffer)?;
        let mut metadata_buffer = String::new();
        archive
            .by_name("metadata.toml")?
            .read_to_string(&mut metadata_buffer)?;
        let metadata = toml::from_str::<BrushPresetMetadata>(&metadata_buffer)?;

        let mut required_spacing_graph_buffer = String::new();
        archive
            .by_name("required_spacing.csg")?
            .read_to_string(&mut required_spacing_graph_buffer)?;
        let required_spacing_graph =
            toml::from_str::<SerializableGraph>(&required_spacing_graph_buffer)?;

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

        let mut stroke_postprocess_graphs = Vec::new();
        let files = archive
            .file_names()
            .map(ToString::to_string)
            .collect::<Vec<_>>();
        for file in files {
            if file.starts_with("stroke_postprocess/") && file != "stroke_postprocess/" {
                let mut buf = String::new();
                archive.by_name(&file)?.read_to_string(&mut buf)?;
                let graph = toml::from_str::<SerializableGraph>(&buf)?;
                stroke_postprocess_graphs.push(graph);
            }
        }

        Ok(BrushPreset {
            metadata,
            required_spacing_graph,
            main_graph,
            stroke_postprocess_graphs,
            external_vars,
        })
    }

    fn write(
        &self,
        asset: &Self::Asset,
        writer: &mut dyn std::io::Write,
    ) -> Result<(), Self::Error> {
        let mut buf = Vec::new();
        let mut zip = ZipWriter::new(Cursor::new(&mut buf));

        zip.start_file("required_spacing.csg", FileOptions::<()>::default())?;
        let required_spacing_graph_buffer = toml::to_string(&asset.required_spacing_graph)?;
        zip.write_all(required_spacing_graph_buffer.as_bytes())?;

        zip.start_file("main.csg", FileOptions::<()>::default())?;
        let main_graph_buffer = toml::to_string(&asset.main_graph)?;
        zip.write_all(main_graph_buffer.as_bytes())?;

        zip.start_file("metadata.toml", FileOptions::<()>::default())?;
        let metadata_buffer = toml::to_string(&asset.metadata)?;
        zip.write_all(metadata_buffer.as_bytes())?;

        if !asset.external_vars.is_empty() {
            zip.start_file("external_vars.toml", FileOptions::<()>::default())?;
            let external_vars_buffer = toml::Value::try_from(&asset.external_vars)?.to_string();
            zip.write_all(external_vars_buffer.as_bytes())?;
        }

        for (i, graph) in asset.stroke_postprocess_graphs.iter().enumerate() {
            zip.start_file(
                format!("stroke_postprocess/{}.csg", i),
                FileOptions::<()>::default(),
            )?;
            let graph_buffer = toml::to_string(graph)?;
            zip.write_all(graph_buffer.as_bytes())?;
        }

        zip.finish()?;
        writer.write_all(&buf)?;

        Ok(())
    }
}
