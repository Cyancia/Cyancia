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

use crate::desc::wgsl::{BrushPose, ColorJitter, Dynamics};

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
    _pattern_assets: &HashMap<Uuid, AssetId<Image>>,
) -> Result<BrushPreset> {
    let (opacity, flow, pressure_overrides_size, pressure_overrides_opacity) =
        match &brush.tool_options {
            Some(ToolOptions::Paint(options)) => (
                (options.opacity as f32 / 100.0).clamp(0.0, 1.0),
                (options.flow as f32 / 100.0).clamp(0.0, 1.0),
                options.use_pressure_overrides_size,
                options.use_pressure_overrides_opacity,
            ),
            _ => (1.0, 1.0, false, false),
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
    let hue_jitter = brush
        .hue_jitter
        .as_ref()
        .map(|value| value.value as f32 / 100.0)
        .unwrap_or(0.0)
        .clamp(0.0, 1.0);
    let saturation_jitter = brush
        .saturation_jitter
        .as_ref()
        .map(|value| value.value as f32 / 100.0)
        .unwrap_or(0.0)
        .clamp(0.0, 1.0);
    let value_jitter = brush
        .value_jitter
        .as_ref()
        .map(|value| value.value as f32 / 100.0)
        .unwrap_or(0.0)
        .clamp(0.0, 1.0);
    let color_jitter = (hue_jitter > 0.0 || saturation_jitter > 0.0 || value_jitter > 0.0)
        .then_some(ColorJitter {
            hue: hue_jitter,
            saturation: saturation_jitter,
            value: value_jitter,
        });

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
                color_jitter,
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
                color_jitter,
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
