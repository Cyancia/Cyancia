use std::collections::HashMap;

use anyhow::{Context, Result, bail, ensure};
use lapiz_abr::{BrushPreset as AbrBrushPreset, BrushTip};
use lapiz_assets::asset::AssetId;
use lapiz_brush::asset::BrushPreset;
use lapiz_render::texture::Image;
use uuid::Uuid;

pub mod graph;
pub mod wgsl;

pub fn parse_desc(
    brush: &AbrBrushPreset,
    sample_assets: &HashMap<Uuid, AssetId<Image>>,
    _pattern_assets: &HashMap<Uuid, AssetId<Image>>,
) -> Result<BrushPreset> {
    let (required_spacing_graph, main_graph) = match &brush.brush {
        BrushTip::Computed(tip) => {
            ensure!(tip.interpolation, "unsupported computed interpolation");
            graph::computed_graphs(
                tip.diameter.value as f32,
                (tip.hardness.value / 100.0).clamp(0.0, 1.0) as f32,
                tip.angle.value.to_radians() as f32,
                (tip.roundness.value / 100.0).clamp(0.001, 1.0) as f32,
                (tip.spacing.value / 100.0).max(0.001) as f32,
                tip.flip_x ^ brush.flip_x,
                tip.flip_y ^ brush.flip_y,
            )?
        }
        BrushTip::Sampled(tip) => {
            ensure!(tip.interpolation, "unsupported sampled interpolation");
            let sample_asset = sample_assets
                .get(&tip.id)
                .copied()
                .with_context(|| format!("sample not found {}", tip.id))?;
            graph::sampled_graphs(
                sample_asset,
                tip.diameter.value as f32,
                tip.angle.value.to_radians() as f32,
                (tip.roundness.value / 100.0).clamp(0.001, 1.0) as f32,
                (tip.spacing.value / 100.0).max(0.001) as f32,
                tip.flip_x ^ brush.flip_x,
                tip.flip_y ^ brush.flip_y,
            )?
        }
        BrushTip::DBrush(_) => bail!("unsupported dual brush tip in {}", brush.name),
    };

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
