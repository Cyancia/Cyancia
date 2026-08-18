use std::collections::HashMap;

use anyhow::{Result, bail};
use lapiz_abr::{BrushPreset as AbrBrushPreset, BrushTip};
use lapiz_assets::asset::AssetId;
use lapiz_brush::asset::BrushPreset;
use lapiz_render::texture::Image;
use uuid::Uuid;

pub mod graph;
pub mod wgsl;

pub fn parse_desc(
    brush: &AbrBrushPreset,
    _sample_assets: &HashMap<Uuid, AssetId<Image>>,
    _pattern_assets: &HashMap<Uuid, AssetId<Image>>,
) -> Result<BrushPreset> {
    let BrushTip::Computed(tip) = &brush.brush else {
        bail!("unsupported brush tip in {}", brush.name);
    };

    let (required_spacing_graph, main_graph) = graph::computed_graphs(
        tip.diameter.value as f32,
        (tip.hardness.value / 100.0).clamp(0.0, 1.0) as f32,
        tip.angle.value.to_radians() as f32,
        (tip.roundness.value / 100.0).clamp(0.0, 1.0) as f32,
        (tip.spacing.value / 100.0).max(0.001) as f32,
        brush.flip_x,
        brush.flip_y,
    )?;

    Ok(BrushPreset {
        metadata: lapiz_brush::asset::BrushPresetMetadata {
            name: brush.name.clone(),
        },
        required_spacing_graph,
        main_graph,
        stroke_postprocess_graphs: Vec::new(),
        external_vars: Vec::new(),
    })
}
