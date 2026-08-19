use anyhow::{Result, bail};
use lapiz_abr_derive::{AbrClass, AbrEnum, AbrIntegerEnum, AbrObject};
use uuid::Uuid;

use crate::{Cursor, UnitFloat};

#[derive(Debug, AbrClass)]
#[abr(class = "null")]
pub(crate) struct BrushDescriptorRoot {
    #[abr(key = "Brsh")]
    pub brushes: Vec<Descriptor>,
}

#[derive(Debug, AbrObject)]
pub enum BrushTip {
    Sampled(SampledBrushTip),
    Computed(ComputedBrushTip),
    DBrush(DBrushTip),
}

#[derive(Debug, AbrClass)]
#[abr(class = "sampledBrush")]
pub struct SampledBrushTip {
    #[abr(key = "Nm  ")]
    pub name: String,
    #[abr(key = "sampledData")]
    pub id: Uuid,
    #[abr(key = "Dmtr")]
    pub diameter: UnitFloat,
    #[abr(key = "Angl")]
    pub angle: UnitFloat,
    #[abr(key = "Rndn")]
    pub roundness: UnitFloat,
    #[abr(key = "Spcn")]
    pub spacing: UnitFloat,
    #[abr(key = "Intr")]
    pub interpolation: bool,
    #[abr(key = "flipX")]
    pub flip_x: bool,
    #[abr(key = "flipY")]
    pub flip_y: bool,
}

#[derive(Debug, AbrClass)]
#[abr(class = "computedBrush")]
pub struct ComputedBrushTip {
    #[abr(key = "Dmtr")]
    pub diameter: UnitFloat,
    #[abr(key = "Hrdn")]
    pub hardness: UnitFloat,
    #[abr(key = "Angl")]
    pub angle: UnitFloat,
    #[abr(key = "Rndn")]
    pub roundness: UnitFloat,
    #[abr(key = "Spcn")]
    pub spacing: UnitFloat,
    #[abr(key = "Intr")]
    pub interpolation: bool,
    #[abr(key = "flipX")]
    pub flip_x: bool,
    #[abr(key = "flipY")]
    pub flip_y: bool,
}

#[derive(Debug, AbrClass)]
#[abr(class = "dBrush")]
pub struct DBrushTip {
    #[abr(key = "Dmtr")]
    pub diameter: UnitFloat,
    #[abr(key = "Angl")]
    pub angle: UnitFloat,
    #[abr(key = "Spcn")]
    pub spacing: UnitFloat,
    #[abr(key = "Dnst")]
    pub density: UnitFloat,
    #[abr(key = "Lngt")]
    pub length: UnitFloat,
    #[abr(key = "clumping")]
    pub clumping: UnitFloat,
    #[abr(key = "thickness")]
    pub thickness: UnitFloat,
    #[abr(key = "stiffness")]
    pub stiffness: UnitFloat,
    #[abr(key = "Shp ")]
    pub shape: i32,
    #[abr(key = "physics")]
    pub physics: bool,
    #[abr(key = "Intr")]
    pub interpolation: bool,
    #[abr(key = "flipX")]
    pub flip_x: bool,
    #[abr(key = "flipY")]
    pub flip_y: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, AbrIntegerEnum)]
pub enum DynamicsControl {
    #[abr(value = 0)]
    Off,
    #[abr(value = 1)]
    Fade,
    #[abr(value = 2)]
    PenPressure,
    #[abr(value = 3)]
    PenTilt,
    #[abr(value = 4)]
    StylusWheel,
    #[abr(value = 5)]
    InitialDirection,
    #[abr(value = 6)]
    Direction,
    #[abr(value = 8)]
    Rotation,
}

#[derive(Debug, AbrClass)]
#[abr(class = "brVr")]
pub struct PropertyDynamics {
    #[abr(key = "bVTy")]
    pub control: DynamicsControl,
    #[abr(key = "fStp")]
    pub fade_steps: i32,
    #[abr(key = "jitter")]
    pub jitter: UnitFloat,
    #[abr(key = "Mnm ")]
    pub minimum: Option<UnitFloat>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, AbrEnum)]
#[abr(enum_type = "BlnM")]
pub enum BlendMode {
    #[abr(value = "Nrml")]
    Normal,
    #[abr(value = "Dslv")]
    Dissolve,
    #[abr(value = "Bhnd")]
    Behind,
    #[abr(value = "Clar")]
    Clear,
    #[abr(value = "Drkn")]
    Darken,
    #[abr(value = "Mltp")]
    Multiply,
    #[abr(value = "CBrn")]
    ColorBurn,
    #[abr(value = "linearBurn")]
    LinearBurn,
    #[abr(value = "darkerColor")]
    DarkerColor,
    #[abr(value = "Lghn")]
    Lighten,
    #[abr(value = "Scrn")]
    Screen,
    #[abr(value = "CDdg")]
    ColorDodge,
    #[abr(value = "linearDodge")]
    LinearDodge,
    #[abr(value = "lighterColor")]
    LighterColor,
    #[abr(value = "Ovrl")]
    Overlay,
    #[abr(value = "SftL")]
    SoftLight,
    #[abr(value = "HrdL")]
    HardLight,
    #[abr(value = "vividLight")]
    VividLight,
    #[abr(value = "linearLight")]
    LinearLight,
    #[abr(value = "pinLight")]
    PinLight,
    #[abr(value = "hardMix")]
    HardMix,
    #[abr(value = "Dfrn")]
    Difference,
    #[abr(value = "Xclu")]
    Exclusion,
    #[abr(value = "blendSubtraction")]
    Subtract,
    #[abr(value = "blendDivide")]
    Divide,
    #[abr(value = "H   ")]
    Hue,
    #[abr(value = "Strt")]
    Saturation,
    #[abr(value = "Clr ")]
    Color,
    #[abr(value = "Lmns")]
    Luminosity,
    #[abr(value = "Hght")]
    Height,
    #[abr(value = "Sbtr")]
    SubtractTexture,
    #[abr(value = "linearHeight")]
    LinearHeight,
}

#[derive(Debug, AbrClass)]
#[abr(class = "Ptrn")]
pub struct PatternReference {
    #[abr(key = "Nm  ")]
    pub name: String,
    #[abr(key = "Idnt")]
    pub id: Uuid,
}

#[derive(Debug, AbrClass)]
#[abr(class = "dualBrush")]
pub struct DualBrush {
    #[abr(key = "useDualBrush")]
    pub enabled: bool,
    #[abr(key = "Flip")]
    #[abr(default = false)]
    pub flip: bool,
    #[abr(key = "Brsh")]
    pub brush: Option<BrushTip>,
    #[abr(key = "BlnM")]
    pub blend_mode: Option<BlendMode>,
    #[abr(key = "useScatter")]
    #[abr(default = false)]
    pub use_scatter: bool,
    #[abr(key = "Spcn")]
    pub spacing: Option<UnitFloat>,
    #[abr(key = "Cnt ")]
    #[abr(default = 1.0)]
    pub scatter_count: f64,
    #[abr(key = "bothAxes")]
    #[abr(default = false)]
    pub scatter_both_axes: bool,
    #[abr(key = "countDynamics")]
    pub count_dynamics: Option<PropertyDynamics>,
    #[abr(key = "scatterDynamics")]
    pub scatter_dynamics: Option<PropertyDynamics>,
}

#[derive(Debug, AbrClass)]
#[abr(class = "brushGroup")]
pub struct BrushGroup {
    #[abr(key = "useBrushGroup")]
    pub enabled: bool,
}

#[derive(Debug, AbrClass)]
#[abr(class = "RGBC")]
pub struct RgbColor {
    #[abr(key = "Rd  ")]
    pub red: f64,
    #[abr(key = "Grn ")]
    pub green: f64,
    #[abr(key = "Bl  ")]
    pub blue: f64,
}

#[derive(Debug, AbrObject)]
pub enum ToolOptions {
    Paint(PaintToolOptions),
    Smudge(SmudgeToolOptions),
    Sh(ShToolOptions),
    Eraser(EraserToolOptions),
}

#[derive(Debug, AbrClass)]
#[abr(class = "PbTl")]
pub struct PaintToolOptions {
    #[abr(key = "brushPreset")]
    pub brush_preset: bool,
    #[abr(key = "Md  ")]
    #[abr(default = BlendMode::Normal)]
    pub blend_mode: BlendMode,
    #[abr(key = "Opct")]
    #[abr(default = 100)]
    pub opacity: i32,
    #[abr(key = "flow")]
    #[abr(default = 100)]
    pub flow: i32,
    #[abr(key = "FrgC")]
    pub foreground_color: Option<RgbColor>,
    #[abr(key = "Smoo")]
    #[abr(default = 0)]
    pub smoo: i32,
    #[abr(key = "clVr")]
    pub color_dynamics: Option<PropertyDynamics>,
    #[abr(key = "opVr")]
    pub opacity_dynamics: Option<PropertyDynamics>,
    #[abr(key = "prVr")]
    pub flow_dynamics: Option<PropertyDynamics>,
    #[abr(key = "szVr")]
    pub size_dynamics: Option<PropertyDynamics>,
    #[abr(key = "pressureSmoothing")]
    #[abr(default = false)]
    pub pressure_smoothing: bool,
    #[abr(key = "smoothing")]
    #[abr(default = false)]
    pub smoothing: bool,
    #[abr(key = "smoothingCatchup")]
    #[abr(default = true)]
    pub smoothing_catchup: bool,
    #[abr(key = "smoothingCatchupAtEnd")]
    #[abr(default = false)]
    pub smoothing_catchup_at_end: bool,
    #[abr(key = "smoothingRadiusMode")]
    #[abr(default = false)]
    pub smoothing_radius_mode: bool,
    #[abr(key = "smoothingValue")]
    #[abr(default = 0.0)]
    pub smoothing_value: f64,
    #[abr(key = "smoothingZoomCompensation")]
    #[abr(default = true)]
    pub smoothing_zoom_compensation: bool,
    #[abr(key = "useLegacy")]
    #[abr(default = false)]
    pub use_legacy: bool,
    #[abr(key = "usePressureOverridesOpacity")]
    #[abr(default = false)]
    pub use_pressure_overrides_opacity: bool,
    #[abr(key = "usePressureOverridesSize")]
    #[abr(default = false)]
    pub use_pressure_overrides_size: bool,
}

#[derive(Debug, AbrClass)]
#[abr(class = "SmTl")]
pub struct SmudgeToolOptions {
    #[abr(key = "brushPreset")]
    pub brush_preset: bool,
    #[abr(key = "Md  ")]
    #[abr(default = BlendMode::Normal)]
    pub blend_mode: BlendMode,
    #[abr(key = "Prs ")]
    pub strength: i32,
    #[abr(key = "SmdF")]
    pub smd_f: bool,
    #[abr(key = "SmdS")]
    pub smd_s: bool,
    #[abr(key = "prVr")]
    pub flow_dynamics: Option<PropertyDynamics>,
    #[abr(key = "szVr")]
    pub size_dynamics: Option<PropertyDynamics>,
    #[abr(key = "pressureSmoothing")]
    #[abr(default = false)]
    pub pressure_smoothing: bool,
    #[abr(key = "smoothing")]
    #[abr(default = false)]
    pub smoothing: bool,
    #[abr(key = "smoothingCatchup")]
    #[abr(default = true)]
    pub smoothing_catchup: bool,
    #[abr(key = "smoothingCatchupAtEnd")]
    #[abr(default = false)]
    pub smoothing_catchup_at_end: bool,
    #[abr(key = "smoothingRadiusMode")]
    #[abr(default = false)]
    pub smoothing_radius_mode: bool,
    #[abr(key = "smoothingValue")]
    #[abr(default = 0.0)]
    pub smoothing_value: f64,
    #[abr(key = "smoothingZoomCompensation")]
    #[abr(default = true)]
    pub smoothing_zoom_compensation: bool,
    #[abr(key = "useLegacy")]
    #[abr(default = false)]
    pub use_legacy: bool,
    #[abr(key = "usePressureOverridesOpacity")]
    #[abr(default = false)]
    pub use_pressure_overrides_opacity: bool,
    #[abr(key = "usePressureOverridesSize")]
    #[abr(default = false)]
    pub use_pressure_overrides_size: bool,
}

#[derive(Debug, AbrClass)]
#[abr(class = "ShTl")]
pub struct ShToolOptions {
    #[abr(key = "brushPreset")]
    pub brush_preset: bool,
    #[abr(key = "BlrS")]
    pub blr_s: bool,
    #[abr(key = "Md  ")]
    pub blend_mode: BlendMode,
    #[abr(key = "detailBoost")]
    pub detail_boost: bool,
    #[abr(key = "flow")]
    pub flow: i32,
    #[abr(key = "prVr")]
    pub flow_dynamics: PropertyDynamics,
    #[abr(key = "smoothing")]
    pub smoothing: bool,
    #[abr(key = "szVr")]
    pub size_dynamics: PropertyDynamics,
    #[abr(key = "useLegacy")]
    pub use_legacy: bool,
    #[abr(key = "usePressureOverridesOpacity")]
    pub use_pressure_overrides_opacity: bool,
    #[abr(key = "usePressureOverridesSize")]
    pub use_pressure_overrides_size: bool,
}

#[derive(Debug, AbrClass)]
#[abr(class = "ErTl")]
pub struct EraserToolOptions {
    #[abr(key = "brushPreset")]
    pub brush_preset: bool,
    #[abr(key = "ErsB")]
    pub eraser_behavior: i32,
    #[abr(key = "MgcE")]
    pub magic_eraser: bool,
    #[abr(key = "Opct")]
    #[abr(default = 100)]
    pub opacity: i32,
    #[abr(key = "flow")]
    #[abr(default = 100)]
    pub flow: i32,
    #[abr(key = "Smoo")]
    pub smoo: i32,
    #[abr(key = "pressureSmoothing")]
    pub pressure_smoothing: bool,
    #[abr(key = "smoothing")]
    pub smoothing: bool,
    #[abr(key = "smoothingCatchup")]
    pub smoothing_catchup: bool,
    #[abr(key = "smoothingCatchupAtEnd")]
    pub smoothing_catchup_at_end: bool,
    #[abr(key = "smoothingRadiusMode")]
    pub smoothing_radius_mode: bool,
    #[abr(key = "smoothingValue")]
    pub smoothing_value: f64,
    #[abr(key = "smoothingZoomCompensation")]
    pub smoothing_zoom_compensation: bool,
    #[abr(key = "useLegacy")]
    pub use_legacy: bool,
    #[abr(key = "usePressureOverridesOpacity")]
    pub use_pressure_overrides_opacity: bool,
    #[abr(key = "usePressureOverridesSize")]
    pub use_pressure_overrides_size: bool,
}

#[derive(Debug, AbrClass)]
#[abr(class = "brushPreset")]
pub struct Descriptor {
    #[abr(key = "Nm  ")]
    pub name: String,
    #[abr(key = "Brsh")]
    pub brush: BrushTip,
    #[abr(key = "Nose")]
    pub noise: bool,
    #[abr(key = "Rpt ")]
    pub repeat: bool,
    #[abr(key = "Wtdg")]
    pub wtdg: bool,
    #[abr(key = "useBrushPose")]
    pub use_brush_pose: bool,
    #[abr(key = "useBrushSize")]
    pub use_brush_size: bool,
    #[abr(key = "useTipDynamics")]
    pub use_tip_dynamics: bool,
    #[abr(key = "flipX")]
    #[abr(default = false)]
    pub flip_x: bool,
    #[abr(key = "flipY")]
    #[abr(default = false)]
    pub flip_y: bool,
    #[abr(key = "brushProjection")]
    #[abr(default = false)]
    pub brush_projection: bool,
    #[abr(key = "minimumDiameter")]
    pub minimum_diameter: Option<UnitFloat>,
    #[abr(key = "minimumRoundness")]
    pub minimum_roundness: Option<UnitFloat>,
    #[abr(key = "szVr")]
    pub size_dynamics: Option<PropertyDynamics>,
    #[abr(key = "angleDynamics")]
    pub angle_dynamics: Option<PropertyDynamics>,
    #[abr(key = "roundnessDynamics")]
    pub roundness_dynamics: Option<PropertyDynamics>,
    #[abr(key = "tiltScale")]
    pub tilt_scale: Option<UnitFloat>,
    #[abr(key = "usePaintDynamics")]
    pub use_paint_dynamics: bool,
    #[abr(key = "opVr")]
    pub opacity_dynamics: Option<PropertyDynamics>,
    #[abr(key = "prVr")]
    pub flow_dynamics: Option<PropertyDynamics>,
    #[abr(key = "wtVr")]
    pub wetness_dynamics: Option<PropertyDynamics>,
    #[abr(key = "mxVr")]
    pub mix_dynamics: Option<PropertyDynamics>,
    #[abr(key = "useColorDynamics")]
    pub use_color_dynamics: bool,
    #[abr(key = "clVr")]
    pub color_dynamics: Option<PropertyDynamics>,
    #[abr(key = "H   ")]
    pub hue_jitter: Option<UnitFloat>,
    #[abr(key = "Strt")]
    pub saturation_jitter: Option<UnitFloat>,
    #[abr(key = "Brgh")]
    pub value_jitter: Option<UnitFloat>,
    #[abr(key = "purity")]
    pub purity_jitter: Option<UnitFloat>,
    #[abr(key = "colorDynamicsPerTip")]
    #[abr(default = false)]
    pub color_dynamics_per_tip: bool,
    #[abr(key = "useScatter")]
    pub use_scatter: bool,
    #[abr(key = "Spcn")]
    pub scatter_spacing: Option<UnitFloat>,
    #[abr(key = "Cnt ")]
    #[abr(default = 1.0)]
    pub scatter_count: f64,
    #[abr(key = "bothAxes")]
    #[abr(default = false)]
    pub scatter_both_axes: bool,
    #[abr(key = "countDynamics")]
    pub count_dynamics: Option<PropertyDynamics>,
    #[abr(key = "scatterDynamics")]
    pub scatter_dynamics: Option<PropertyDynamics>,
    #[abr(key = "useTexture")]
    pub use_texture: bool,
    #[abr(key = "Txtr")]
    pub texture: Option<PatternReference>,
    #[abr(key = "TxtC")]
    #[abr(default = false)]
    pub txt_c: bool,
    #[abr(key = "interpretation")]
    pub interpretation: Option<bool>,
    #[abr(key = "textureBlendMode")]
    pub texture_blend_mode: Option<BlendMode>,
    #[abr(key = "textureDepth")]
    pub texture_depth: Option<UnitFloat>,
    #[abr(key = "minimumDepth")]
    pub texture_minimum_depth: Option<UnitFloat>,
    #[abr(key = "textureDepthDynamics")]
    pub texture_depth_dynamics: Option<PropertyDynamics>,
    #[abr(key = "textureScale")]
    pub texture_scale: Option<UnitFloat>,
    #[abr(key = "InvT")]
    #[abr(default = false)]
    pub texture_inverted: bool,
    #[abr(key = "protectTexture")]
    #[abr(default = false)]
    pub protect_texture: bool,
    #[abr(key = "textureBrightness")]
    #[abr(default = 0)]
    pub texture_brightness: i32,
    #[abr(key = "textureContrast")]
    #[abr(default = 0)]
    pub texture_contrast: i32,
    #[abr(key = "brushPoseAngle")]
    #[abr(default = 0)]
    pub brush_pose_angle: i32,
    #[abr(key = "brushPosePressure")]
    pub brush_pose_pressure: Option<UnitFloat>,
    #[abr(key = "brushPoseTiltX")]
    #[abr(default = 0)]
    pub brush_pose_tilt_x: i32,
    #[abr(key = "brushPoseTiltY")]
    #[abr(default = 0)]
    pub brush_pose_tilt_y: i32,
    #[abr(key = "overridePoseAngle")]
    #[abr(default = false)]
    pub override_pose_angle: bool,
    #[abr(key = "overridePosePressure")]
    #[abr(default = false)]
    pub override_pose_pressure: bool,
    #[abr(key = "overridePoseTiltX")]
    #[abr(default = false)]
    pub override_pose_tilt_x: bool,
    #[abr(key = "overridePoseTiltY")]
    #[abr(default = false)]
    pub override_pose_tilt_y: bool,
    #[abr(key = "dualBrush")]
    pub dual_brush: DualBrush,
    #[abr(key = "brushGroup")]
    pub brush_group: BrushGroup,
    #[abr(key = "toolOptions")]
    pub tool_options: Option<ToolOptions>,
}

impl BrushDescriptorRoot {
    pub(crate) fn parse_desc_section(cursor: &mut Cursor<'_>) -> Result<Vec<Descriptor>> {
        let version = cursor.read_u32_be()?;
        if version != 16 {
            bail!("unsupported ABR descriptor version {version}");
        }

        let root = <Self as crate::descriptor::AbrObject>::parse(cursor)?;
        if cursor.remaining() != 0 {
            bail!("unexpected trailing ABR descriptor data");
        }

        Ok(root.brushes)
    }
}
