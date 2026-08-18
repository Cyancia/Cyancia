use std::collections::HashMap;

use anyhow::{Context, Result, bail, ensure};
use lapiz_abr::{
    BrushPreset as AbrBrushPreset, BrushTip, DynamicsControl,
    PropertyDynamics as AbrPropertyDynamics, ToolOptions,
};
use lapiz_assets::asset::AssetId;
use lapiz_brush::asset::BrushPreset;
use lapiz_render::texture::Image;
use uuid::Uuid;

use crate::desc::wgsl::{BrushPose, BrushTexture, ColorAdjustment, Dynamics, Scatter};

pub mod graph;
pub mod wgsl;

fn parse_dynamics(
    dynamics: &AbrPropertyDynamics,
    pressure_override: bool,
    minimum: f32,
) -> Result<Dynamics> {
    let control = if pressure_override {
        DynamicsControl::PenPressure
    } else {
        dynamics.control
    };
    ensure!(
        control != DynamicsControl::StylusWheel,
        "unsupported stylus wheel"
    );
    let dynamics_minimum = dynamics
        .minimum
        .as_ref()
        .map(|value| value.value as f32 / 100.0)
        .unwrap_or(0.0);
    Ok(Dynamics {
        control,
        fade_steps: dynamics.fade_steps,
        jitter: (dynamics.jitter.value as f32 / 100.0).clamp(0.0, 1.0),
        minimum: minimum.max(dynamics_minimum).clamp(0.0, 1.0),
    })
}

pub fn parse_desc(
    brush: &AbrBrushPreset,
    sample_assets: &HashMap<Uuid, AssetId<Image>>,
    pattern_assets: &HashMap<Uuid, AssetId<Image>>,
) -> Result<BrushPreset> {
    let paint_options = match &brush.tool_options {
        Some(ToolOptions::Paint(options)) => Some(options),
        _ => None,
    };
    let (opacity, flow, pressure_overrides_size, pressure_overrides_opacity) = match paint_options {
        Some(options) => (
            (options.opacity as f32 / 100.0).clamp(0.0, 1.0),
            (options.flow as f32 / 100.0).clamp(0.0, 1.0),
            options.use_pressure_overrides_size,
            options.use_pressure_overrides_opacity,
        ),
        None => (1.0, 1.0, false, false),
    };
    let minimum_diameter = brush
        .minimum_diameter
        .as_ref()
        .map(|value| value.value as f32 / 100.0)
        .unwrap_or(0.0);
    let size_dynamics = brush
        .size_dynamics
        .as_ref()
        .map(|dynamics| {
            parse_dynamics(dynamics, pressure_overrides_size, minimum_diameter)
                .context("size dynamics")
        })
        .transpose()?;
    let size_dynamics = if pressure_overrides_size && size_dynamics.is_none() {
        Some(Dynamics {
            control: DynamicsControl::PenPressure,
            fade_steps: 0,
            jitter: 0.0,
            minimum: minimum_diameter.clamp(0.0, 1.0),
        })
    } else {
        size_dynamics
    };
    let opacity_dynamics = brush
        .opacity_dynamics
        .as_ref()
        .map(|dynamics| {
            parse_dynamics(dynamics, pressure_overrides_opacity, 0.0).context("opacity dynamics")
        })
        .transpose()?;
    let opacity_dynamics = if pressure_overrides_opacity && opacity_dynamics.is_none() {
        Some(Dynamics {
            control: DynamicsControl::PenPressure,
            fade_steps: 0,
            jitter: 0.0,
            minimum: 0.0,
        })
    } else {
        opacity_dynamics
    };
    let flow_dynamics = brush
        .flow_dynamics
        .as_ref()
        .map(|dynamics| parse_dynamics(dynamics, false, 0.0).context("flow dynamics"))
        .transpose()?;
    let angle_dynamics = brush
        .angle_dynamics
        .as_ref()
        .map(|dynamics| {
            ensure!(
                dynamics.control != DynamicsControl::Fade,
                "unsupported angle fade"
            );
            parse_dynamics(dynamics, false, 0.0).context("angle dynamics")
        })
        .transpose()?;
    let minimum_roundness = brush
        .minimum_roundness
        .as_ref()
        .map(|value| value.value as f32 / 100.0)
        .unwrap_or(0.0);
    let roundness_dynamics = brush
        .roundness_dynamics
        .as_ref()
        .map(|dynamics| {
            parse_dynamics(dynamics, false, minimum_roundness).context("roundness dynamics")
        })
        .transpose()?;
    let tilt_scale = brush
        .tilt_scale
        .as_ref()
        .map(|value| value.value as f32 / 100.0)
        .unwrap_or(2.0)
        .clamp(0.0, 2.0);
    let pose = if brush.use_brush_pose {
        BrushPose {
            pressure: brush
                .override_pose_pressure
                .then(|| brush.brush_pose_pressure.as_ref())
                .flatten()
                .map(|value| (value.value as f32 / 100.0).clamp(0.0, 1.0)),
            azimuth: brush
                .override_pose_angle
                .then(|| (brush.brush_pose_angle as f32).to_radians()),
            tilt_x: brush
                .override_pose_tilt_x
                .then(|| (brush.brush_pose_tilt_x as f32).to_radians()),
            tilt_y: brush
                .override_pose_tilt_y
                .then(|| (brush.brush_pose_tilt_y as f32).to_radians()),
        }
    } else {
        BrushPose::default()
    };
    let color_enabled = brush.use_color_dynamics;
    let hue_jitter = if color_enabled {
        brush
            .hue_jitter
            .as_ref()
            .map(|value| value.value as f32 / 100.0)
            .unwrap_or(0.0)
            .clamp(0.0, 1.0)
    } else {
        0.0
    };
    let saturation_jitter = if color_enabled {
        brush
            .saturation_jitter
            .as_ref()
            .map(|value| value.value as f32 / 100.0)
            .unwrap_or(0.0)
            .clamp(0.0, 1.0)
    } else {
        0.0
    };
    let value_jitter = if color_enabled {
        brush
            .value_jitter
            .as_ref()
            .map(|value| value.value as f32 / 100.0)
            .unwrap_or(0.0)
            .clamp(0.0, 1.0)
    } else {
        0.0
    };
    let purity = if color_enabled {
        brush
            .purity_jitter
            .as_ref()
            .map(|value| value.value as f32 / 100.0)
            .unwrap_or(0.0)
            .clamp(-1.0, 1.0)
    } else {
        0.0
    };
    let color_dynamics = if color_enabled {
        brush
            .color_dynamics
            .as_ref()
            .or_else(|| paint_options.and_then(|options| options.color_dynamics.as_ref()))
            .map(|dynamics| parse_dynamics(dynamics, false, 0.0).context("color dynamics"))
            .transpose()?
    } else {
        None
    };
    let foreground_color = paint_options
        .and_then(|options| options.foreground_color.as_ref())
        .map(|color| {
            [
                (color.red as f32 / 255.0).clamp(0.0, 1.0),
                (color.green as f32 / 255.0).clamp(0.0, 1.0),
                (color.blue as f32 / 255.0).clamp(0.0, 1.0),
            ]
        });
    let has_hsv_jitter = hue_jitter > 0.0 || saturation_jitter > 0.0 || value_jitter > 0.0;
    ensure!(
        purity == 0.0 || !has_hsv_jitter,
        "unsupported purity with HSV jitter"
    );
    let has_color_adjustment =
        has_hsv_jitter || purity != 0.0 || color_dynamics.is_some() || foreground_color.is_some();
    let color_adjustment = has_color_adjustment.then_some(ColorAdjustment {
        hue_jitter,
        saturation_jitter,
        value_jitter,
        purity,
        dynamics: color_dynamics,
        per_tip: brush.color_dynamics_per_tip,
        foreground_color,
    });
    let scatter = if brush.use_scatter {
        let dynamics = brush
            .scatter_dynamics
            .as_ref()
            .map(|dynamics| parse_dynamics(dynamics, false, 0.0).context("scatter dynamics"))
            .transpose()?;
        let count_dynamics = brush
            .count_dynamics
            .as_ref()
            .map(|dynamics| parse_dynamics(dynamics, false, 0.0).context("count dynamics"))
            .transpose()?;
        let count = brush.scatter_count.round();
        ensure!(
            brush.scatter_count.is_finite()
                && (brush.scatter_count - count).abs() < f64::EPSILON
                && (1.0..=16.0).contains(&count),
            "unsupported scatter count {}",
            brush.scatter_count
        );
        Some(Scatter {
            amount: brush
                .scatter_spacing
                .as_ref()
                .map(|value| value.value as f32 / 100.0)
                .unwrap_or(0.0)
                .max(0.0),
            both_axes: brush.scatter_both_axes,
            dynamics,
            count: count as u32,
            count_dynamics,
        })
    } else {
        None
    };

    ensure!(!brush.protect_texture, "unsupported protect texture");
    ensure!(
        brush.interpretation != Some(false),
        "unsupported texture interpretation"
    );
    ensure!(
        brush.texture_brightness == 0,
        "unsupported texture brightness {}",
        brush.texture_brightness
    );
    ensure!(
        brush.texture_contrast == 0,
        "unsupported texture contrast {}",
        brush.texture_contrast
    );
    let (brush_texture, pattern_asset) = if brush.use_texture {
        let texture = brush
            .texture
            .as_ref()
            .context("missing texture pattern reference")?;
        let pattern_asset = pattern_assets
            .get(&texture.id)
            .copied()
            .with_context(|| format!("missing texture pattern {}", texture.id))?;
        let scale = brush
            .texture_scale
            .as_ref()
            .map(|value| value.value as f32 / 100.0)
            .unwrap_or(1.0);
        ensure!(scale.is_finite() && scale > 0.0, "invalid texture scale");
        let depth = brush
            .texture_depth
            .as_ref()
            .map(|value| value.value as f32 / 100.0)
            .unwrap_or(1.0)
            .clamp(0.0, 1.0);
        let minimum_depth = brush
            .texture_minimum_depth
            .as_ref()
            .map(|value| value.value as f32 / 100.0)
            .unwrap_or(0.0)
            .clamp(0.0, 1.0);
        let depth_dynamics = brush
            .texture_depth_dynamics
            .as_ref()
            .map(|dynamics| {
                parse_dynamics(dynamics, false, minimum_depth).context("texture depth dynamics")
            })
            .transpose()?;
        (
            Some(BrushTexture {
                scale,
                inverted: brush.texture_inverted,
                depth,
                depth_dynamics,
            }),
            Some(pattern_asset),
        )
    } else {
        (None, None)
    };

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
                flow,
                size_dynamics,
                opacity_dynamics,
                flow_dynamics,
                angle_dynamics,
                roundness_dynamics,
                tilt_scale,
                pose,
                color_adjustment,
                scatter,
                brush_texture,
                pattern_asset,
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
                flow,
                size_dynamics,
                opacity_dynamics,
                flow_dynamics,
                angle_dynamics,
                roundness_dynamics,
                tilt_scale,
                pose,
                color_adjustment,
                scatter,
                brush_texture,
                pattern_asset,
            )?
        }
        BrushTip::DBrush(_) => bail!("unsupported dual brush tip in {}", brush.name),
    };

    let stroke_postprocess_graphs = graph::opacity_postprocess_graph(opacity)?
        .into_iter()
        .collect();

    Ok(BrushPreset {
        metadata: lapiz_brush::asset::BrushPresetMetadata {
            name: brush.name.clone(),
        },
        required_spacing_graph,
        main_graph,
        stroke_postprocess_graphs,
        external_vars: Vec::new(),
    })
}
