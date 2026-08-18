use std::sync::LazyLock;

use lapiz_abr::DynamicsControl;
use wesl::syntax::*;
use wesl_quote::{quote_expression, quote_statement};

pub const MAIN_PIXEL_POSITION_INPUT: &str = "pixel_position";
pub const MAIN_PEN_POSITION_INPUT: &str = "pen_position";
pub const MAIN_FOREGROUND_COLOR_INPUT: &str = "foreground_color";
pub const MAIN_BACKGROUND_COLOR_INPUT: &str = "background_color";
pub const MAIN_TIP_TEXTURE_INPUT: &str = "tip_texture";
pub const MAIN_PATTERN_TEXTURE_INPUT: &str = "pattern_texture";
pub const MAIN_COLOR_OUTPUT: &str = "color";
pub const MAIN_BOUNDS_OUTPUT: &str = "bounds";
pub const POSTPROCESS_INPUT_COLOR: &str = "input_color";
pub const POSTPROCESS_STROKE_BOUNDS_INPUT: &str = "stroke_bounds";
pub const REQUIRED_SPACING_OUTPUT: &str = "required_spacing";
pub const PRESSURE_INPUT: &str = "pressure";
pub const TILT_INPUT: &str = "tilt";
pub const AZIMUTH_INPUT: &str = "azimuth";
pub const DIRECTION_INPUT: &str = "direction";
pub const INITIAL_DIRECTION_INPUT: &str = "initial_direction";
pub const DAB_INDEX_INPUT: &str = "dab_index";
pub const STROKE_BEGIN_INPUT: &str = "stroke_begin";

#[derive(Clone, Copy)]
pub struct Dynamics {
    pub control: DynamicsControl,
    pub fade_steps: i32,
    pub jitter: f32,
    pub minimum: f32,
}

#[derive(Clone, Copy, Default)]
pub struct BrushPose {
    pub pressure: Option<f32>,
    pub azimuth: Option<f32>,
    pub tilt_x: Option<f32>,
    pub tilt_y: Option<f32>,
}

#[derive(Clone, Copy)]
pub struct Scatter {
    pub amount: f32,
    pub both_axes: bool,
    pub dynamics: Option<Dynamics>,
    pub count: u32,
    pub count_dynamics: Option<Dynamics>,
}

#[derive(Clone, Copy)]
pub enum TextureBlendMode {
    Multiply,
    Subtract,
    Darken,
    Overlay,
    ColorDodge,
    ColorBurn,
    LinearDodge,
    LinearBurn,
    HardMix,
    Height,
    LinearHeight,
}

#[derive(Clone, Copy)]
pub struct BrushTexture {
    pub scale: f32,
    pub inverted: bool,
    pub depth: f32,
    pub depth_dynamics: Option<Dynamics>,
    pub each_tip: bool,
    pub blend_mode: TextureBlendMode,
}

#[derive(Clone, Copy)]
pub struct ColorAdjustment {
    pub hue_jitter: f32,
    pub saturation_jitter: f32,
    pub value_jitter: f32,
    pub purity: f32,
    pub dynamics: Option<Dynamics>,
    pub per_tip: bool,
    pub foreground_color: Option<[f32; 3]>,
}

// TODO: This is really ugly, can we avoid this?
static MAIN_PIXEL_POSITION_IDENT: LazyLock<Ident> =
    LazyLock::new(|| Ident::new("pixel_position".to_string()));
static MAIN_PEN_POSITION_IDENT: LazyLock<Ident> =
    LazyLock::new(|| Ident::new(MAIN_PEN_POSITION_INPUT.to_string()));
static MAIN_FOREGROUND_COLOR_IDENT: LazyLock<Ident> =
    LazyLock::new(|| Ident::new(MAIN_FOREGROUND_COLOR_INPUT.to_string()));
static MAIN_BACKGROUND_COLOR_IDENT: LazyLock<Ident> =
    LazyLock::new(|| Ident::new(MAIN_BACKGROUND_COLOR_INPUT.to_string()));
static MAIN_TIP_TEXTURE_IDENT: LazyLock<Ident> =
    LazyLock::new(|| Ident::new(MAIN_TIP_TEXTURE_INPUT.to_string()));
static MAIN_PATTERN_TEXTURE_IDENT: LazyLock<Ident> =
    LazyLock::new(|| Ident::new(MAIN_PATTERN_TEXTURE_INPUT.to_string()));
static MAIN_COLOR_IDENT: LazyLock<Ident> =
    LazyLock::new(|| Ident::new(MAIN_COLOR_OUTPUT.to_string()));
static MAIN_BOUNDS_IDENT: LazyLock<Ident> =
    LazyLock::new(|| Ident::new(MAIN_BOUNDS_OUTPUT.to_string()));
static POSTPROCESS_INPUT_COLOR_IDENT: LazyLock<Ident> =
    LazyLock::new(|| Ident::new(POSTPROCESS_INPUT_COLOR.to_string()));
static POSTPROCESS_STROKE_BOUNDS_IDENT: LazyLock<Ident> =
    LazyLock::new(|| Ident::new(POSTPROCESS_STROKE_BOUNDS_INPUT.to_string()));
static REQUIRED_SPACING_IDENT: LazyLock<Ident> =
    LazyLock::new(|| Ident::new(REQUIRED_SPACING_OUTPUT.to_string()));
static PRESSURE_IDENT: LazyLock<Ident> = LazyLock::new(|| Ident::new(PRESSURE_INPUT.to_string()));
static TILT_IDENT: LazyLock<Ident> = LazyLock::new(|| Ident::new(TILT_INPUT.to_string()));
static AZIMUTH_IDENT: LazyLock<Ident> = LazyLock::new(|| Ident::new(AZIMUTH_INPUT.to_string()));
static DIRECTION_IDENT: LazyLock<Ident> = LazyLock::new(|| Ident::new(DIRECTION_INPUT.to_string()));
static INITIAL_DIRECTION_IDENT: LazyLock<Ident> =
    LazyLock::new(|| Ident::new(INITIAL_DIRECTION_INPUT.to_string()));
static DAB_INDEX_IDENT: LazyLock<Ident> = LazyLock::new(|| Ident::new(DAB_INDEX_INPUT.to_string()));
static STROKE_BEGIN_IDENT: LazyLock<Ident> =
    LazyLock::new(|| Ident::new(STROKE_BEGIN_INPUT.to_string()));

fn pose_pressure(pose: BrushPose) -> Expression {
    match pose.pressure {
        Some(pressure) => quote_expression!(#pressure * 1.0),
        None => {
            let pressure = (*PRESSURE_IDENT).clone();
            quote_expression!(#pressure)
        }
    }
}

fn pose_azimuth(pose: BrushPose) -> Expression {
    match pose.azimuth {
        Some(azimuth) => quote_expression!(#azimuth * 1.0),
        None => {
            let azimuth = (*AZIMUTH_IDENT).clone();
            quote_expression!(#azimuth)
        }
    }
}

fn pose_tilt_components(pose: BrushPose) -> (Expression, Expression) {
    let tilt = (*TILT_IDENT).clone();
    let tilt_x = match pose.tilt_x {
        Some(tilt_x) => quote_expression!(#tilt_x * 1.0),
        None => quote_expression!(#tilt.x),
    };
    let tilt_y = match pose.tilt_y {
        Some(tilt_y) => quote_expression!(#tilt_y * 1.0),
        None => quote_expression!(#tilt.y),
    };
    (tilt_x, tilt_y)
}

fn pose_tilt(pose: BrushPose) -> Expression {
    let (tilt_x, tilt_y) = pose_tilt_components(pose);
    quote_expression!(vec2f(#tilt_x, #tilt_y))
}

fn pose_tilt_angle(pose: BrushPose) -> Expression {
    let (tilt_x, tilt_y) = pose_tilt_components(pose);
    quote_expression!(atan2(#tilt_y, #tilt_x))
}

fn dab_random(random_offset: f32) -> Expression {
    let dab_index = (*DAB_INDEX_IDENT).clone();
    quote_expression!(fract(
        sin(f32(#dab_index) * 12.9898 + #random_offset) * 43758.5453
    ))
}

fn stroke_random(random_offset: f32) -> Expression {
    let stroke_begin = (*STROKE_BEGIN_IDENT).clone();
    quote_expression!(fract(
        sin(#stroke_begin * 12.9898 + #random_offset) * 43758.5453
    ))
}

fn scatter_copy_random(random_offset: f32) -> Expression {
    let dab_index = (*DAB_INDEX_IDENT).clone();
    quote_expression!(fract(
        sin(
            f32(#dab_index) * 12.9898
                + f32(copy_index) * 78.233
                + #random_offset
        ) * 43758.5453
    ))
}

fn color_random(per_tip: bool, random_offset: f32) -> Expression {
    if per_tip {
        dab_random(random_offset)
    } else {
        stroke_random(random_offset)
    }
}

fn dynamics_control(dynamics: Dynamics, pose: BrushPose) -> Expression {
    let pressure = pose_pressure(pose);
    let tilt = pose_tilt(pose);
    let azimuth = pose_azimuth(pose);
    let direction = (*DIRECTION_IDENT).clone();
    let initial_direction = (*INITIAL_DIRECTION_IDENT).clone();
    let dab_index = (*DAB_INDEX_IDENT).clone();
    match dynamics.control {
        DynamicsControl::Off => quote_expression!(1.0),
        DynamicsControl::Fade if dynamics.fade_steps <= 0 => quote_expression!(0.0),
        DynamicsControl::Fade => {
            let fade_steps = dynamics.fade_steps as f32;
            quote_expression!(
                1.0 - clamp(f32(#dab_index) / #fade_steps, 0.0, 1.0)
            )
        }
        DynamicsControl::PenPressure => quote_expression!(clamp(#pressure, 0.0, 1.0)),
        DynamicsControl::PenTilt => {
            quote_expression!(clamp(length(#tilt) / render::math::FRAC_PI_2, 0.0, 1.0))
        }
        DynamicsControl::InitialDirection => {
            quote_expression!(clamp(#initial_direction / render::math::TAU + 0.5, 0.0, 1.0))
        }
        DynamicsControl::Direction => {
            quote_expression!(clamp(#direction / render::math::TAU + 0.5, 0.0, 1.0))
        }
        DynamicsControl::Rotation => {
            quote_expression!(fract(#azimuth / render::math::TAU + 1.0))
        }
        DynamicsControl::StylusWheel => unreachable!(),
    }
}

fn dynamics_factor(dynamics: Dynamics, random_offset: f32, pose: BrushPose) -> Expression {
    let control = dynamics_control(dynamics, pose);
    let jitter = dynamics.jitter;
    let minimum = dynamics.minimum;
    let random = dab_random(random_offset);

    quote_expression!(mix(
        mix(#minimum, 1.0, #control),
        #minimum,
        #jitter * #random,
    ))
}

fn color_dynamics_factor(dynamics: Dynamics, per_tip: bool, pose: BrushPose) -> Expression {
    let control = dynamics_control(dynamics, pose);
    let minimum = dynamics.minimum;
    let jitter = dynamics.jitter;
    let random = if per_tip {
        dab_random(89.417)
    } else {
        stroke_random(89.417)
    };
    quote_expression!(mix(
        mix(#minimum, 1.0, #control),
        #minimum,
        #jitter * #random,
    ))
}

fn scatter_offset_expression(scatter: Option<Scatter>, pose: BrushPose) -> Expression {
    let Some(scatter) = scatter else {
        return quote_expression!(vec2f(0.0));
    };
    let factor = match scatter.dynamics {
        Some(dynamics) => dynamics_factor(dynamics, 103.927, pose),
        None => quote_expression!(1.0),
    };
    let amount = scatter.amount;
    let first_random = scatter_copy_random(127.413);
    if scatter.both_axes {
        let second_random = scatter_copy_random(149.819);
        quote_expression!(
            vec2f(
                #first_random * 2.0 - 1.0,
                #second_random * 2.0 - 1.0,
            ) * tip_diameter
                * #amount
                * #factor
        )
    } else {
        let direction = (*DIRECTION_IDENT).clone();
        quote_expression!(
            vec2f(-sin(#direction), cos(#direction))
                * (#first_random * 2.0 - 1.0)
                * tip_diameter
                * #amount
                * #factor
        )
    }
}

fn scatter_count_expression(scatter: Option<Scatter>, pose: BrushPose) -> Expression {
    let Some(scatter) = scatter else {
        return quote_expression!(1u);
    };
    let count = scatter.count.max(1) as f32;
    let Some(dynamics) = scatter.count_dynamics else {
        return quote_expression!(u32(#count));
    };
    let factor = dynamics_factor(dynamics, 173.531, pose);
    quote_expression!(max(
        1u,
        min(u32(#count), u32(round(#count * #factor))),
    ))
}

fn size_diameter_statement(
    diameter: f32,
    dynamics: Option<Dynamics>,
    pose: BrushPose,
) -> Statement {
    let Some(dynamics) = dynamics else {
        return quote_statement! {
            tip_diameter = #diameter;
        };
    };
    let factor = dynamics_factor(dynamics, 78.233, pose);

    quote_statement! {
        tip_diameter = #diameter * #factor;
    }
}

fn tip_roundness_statement(
    roundness: f32,
    dynamics: Option<Dynamics>,
    tilt_scale: f32,
    pose: BrushPose,
) -> Statement {
    let Some(dynamics) = dynamics else {
        return quote_statement! {{
            tip_roundness = #roundness;
            roundness_angle = 0.0;
        }};
    };
    let tilt = pose_tilt(pose);
    let tilt_angle = pose_tilt_angle(pose);
    let (control, angle) = if dynamics.control == DynamicsControl::PenTilt {
        (
            quote_expression!(
                1.0
                    - clamp(
                        length(#tilt) / render::math::FRAC_PI_2 * #tilt_scale,
                        0.0,
                        1.0,
                    )
            ),
            tilt_angle,
        )
    } else {
        (dynamics_control(dynamics, pose), quote_expression!(0.0))
    };
    let jitter = dynamics.jitter;
    let minimum = dynamics.minimum.min(roundness);
    let random = dab_random(91.417);

    quote_statement! {{
        tip_roundness = mix(
            mix(#minimum, #roundness, #control),
            #minimum,
            #jitter * #random,
        );
        roundness_angle = #angle;
    }}
}

fn tip_angle_expression(angle: f32, dynamics: Option<Dynamics>, pose: BrushPose) -> Expression {
    let Some(dynamics) = dynamics else {
        return quote_expression!(#angle + 0.0);
    };
    let pressure = pose_pressure(pose);
    let tilt = pose_tilt(pose);
    let azimuth = pose_azimuth(pose);
    let direction = (*DIRECTION_IDENT).clone();
    let initial_direction = (*INITIAL_DIRECTION_IDENT).clone();
    let minimum = dynamics.minimum;
    let (orientation, amplitude) = match dynamics.control {
        DynamicsControl::Off => (quote_expression!(0.0), quote_expression!(1.0)),
        DynamicsControl::PenPressure => (
            quote_expression!(0.0),
            quote_expression!(mix(
                #minimum,
                1.0,
                clamp(#pressure, 0.0, 1.0),
            )),
        ),
        DynamicsControl::PenTilt => (
            quote_expression!(0.0),
            quote_expression!(mix(
                #minimum,
                1.0,
                clamp(length(#tilt) / render::math::FRAC_PI_2, 0.0, 1.0),
            )),
        ),
        DynamicsControl::InitialDirection => (
            quote_expression!(#initial_direction),
            quote_expression!(1.0),
        ),
        DynamicsControl::Direction => (quote_expression!(#direction), quote_expression!(1.0)),
        DynamicsControl::Rotation => (quote_expression!(#azimuth + 0.0), quote_expression!(1.0)),
        DynamicsControl::Fade | DynamicsControl::StylusWheel => unreachable!(),
    };
    let jitter = dynamics.jitter;
    let random = dab_random(59.719);

    quote_expression!(
        #angle
            + #orientation
            + (#random - 0.5) * render::math::TAU * #jitter * #amplitude
    )
}

fn tip_foreground_statement(adjustment: Option<ColorAdjustment>, pose: BrushPose) -> Statement {
    let runtime_foreground = (*MAIN_FOREGROUND_COLOR_IDENT).clone();
    let Some(adjustment) = adjustment else {
        return quote_statement! {
            tip_foreground = #runtime_foreground;
        };
    };
    let foreground = match adjustment.foreground_color {
        Some([red, green, blue]) => quote_expression!(vec4f(#red, #green, #blue, 1.0)),
        None => quote_expression!(#runtime_foreground),
    };
    let base_foreground = match adjustment.dynamics {
        Some(dynamics) => {
            let background = (*MAIN_BACKGROUND_COLOR_IDENT).clone();
            let factor = color_dynamics_factor(dynamics, adjustment.per_tip, pose);
            quote_expression!(mix(#background, #foreground, #factor))
        }
        None => foreground,
    };
    let hue_offset = if adjustment.hue_jitter > 0.0 {
        let random = color_random(adjustment.per_tip, 17.131);
        let jitter = adjustment.hue_jitter;
        quote_expression!((#random - 0.5) * #jitter)
    } else {
        quote_expression!(0.0)
    };
    let saturation_offset = if adjustment.saturation_jitter > 0.0 {
        let random = color_random(adjustment.per_tip, 21.7);
        let jitter = adjustment.saturation_jitter;
        quote_expression!((#random * 2.0 - 1.0) * #jitter)
    } else {
        quote_expression!(0.0)
    };
    let value_offset = if adjustment.value_jitter > 0.0 {
        let random = color_random(adjustment.per_tip, 73.219);
        let jitter = adjustment.value_jitter;
        quote_expression!((#random * 2.0 - 1.0) * #jitter)
    } else {
        quote_expression!(0.0)
    };
    let purity = adjustment.purity;
    let adjusted_saturation = if purity < 0.0 {
        quote_expression!(jittered_saturation * (1.0 + #purity))
    } else {
        quote_expression!(
            jittered_saturation + (1.0 - jittered_saturation) * #purity
        )
    };

    quote_statement! {{
        let source_foreground = #base_foreground;
        let color_min = min(
            source_foreground.r,
            min(source_foreground.g, source_foreground.b),
        );
        let color_max = max(
            source_foreground.r,
            max(source_foreground.g, source_foreground.b),
        );
        let color_delta = color_max - color_min;
        var hue: f32 = 0.0;
        if color_delta > 0.0 {
            if color_max == source_foreground.r {
                hue = (source_foreground.g - source_foreground.b) / color_delta;
            } else if color_max == source_foreground.g {
                hue = 2.0 + (source_foreground.b - source_foreground.r) / color_delta;
            } else {
                hue = 4.0 + (source_foreground.r - source_foreground.g) / color_delta;
            }
            hue /= 6.0;
        }
        let saturation = select(0.0, color_delta / color_max, color_max > 0.0);
        hue = fract(hue + 1.0 + #hue_offset);
        let jittered_saturation = clamp(
            saturation + #saturation_offset,
            0.0,
            1.0,
        );
        let final_saturation = #adjusted_saturation;
        let jittered_value = clamp(
            color_max + #value_offset,
            0.0,
            1.0,
        );
        let hue_rgb = clamp(
            abs(
                fract(vec3f(hue) + vec3f(1.0, 0.6666666667, 0.3333333333))
                    * 6.0
                    - vec3f(3.0)
            ) - vec3f(1.0),
            vec3f(0.0),
            vec3f(1.0),
        );
        tip_foreground = vec4f(
            jittered_value
                * mix(vec3f(1.0), hue_rgb, final_saturation),
            source_foreground.a,
        );
    }}
}

fn texture_mask_statement(
    texture: Option<BrushTexture>,
    pose: BrushPose,
    base_diameter: f32,
    tip_angle: Expression,
) -> Statement {
    let Some(texture) = texture else {
        return quote_statement! {{}};
    };
    let pattern = (*MAIN_PATTERN_TEXTURE_IDENT).clone();
    let scale = texture.scale;
    let pattern_sample = if texture.each_tip {
        quote_expression!(sample_transformed_local_texture_wrap(
            #pattern,
            pixel_position,
            vec2f(
                #scale * tip_diameter / #base_diameter,
                #scale * tip_diameter * tip_roundness / #base_diameter,
            ),
            #tip_angle + roundness_angle,
            tip_center,
            vec2f(0.5),
        ))
    } else {
        quote_expression!(sample_transformed_local_texture_wrap(
            #pattern,
            pixel_position,
            vec2f(#scale),
            0.0,
            vec2f(0.0),
            vec2f(0.0),
        ))
    };
    let texture_value = if texture.inverted {
        quote_expression!(1.0 - pattern_sample.r)
    } else {
        quote_expression!(pattern_sample.r)
    };
    let depth = texture.depth;
    let effective_depth = match texture.depth_dynamics {
        Some(dynamics) => {
            let control = dynamics_control(dynamics, pose);
            let minimum = dynamics.minimum;
            let jitter = dynamics.jitter;
            let random = dab_random(197.731);
            quote_expression!(mix(
                mix(#minimum, #depth, #control),
                #minimum,
                #jitter * #random,
            ))
        }
        None => quote_expression!(#depth * 1.0),
    };
    let blended_mask = match texture.blend_mode {
        TextureBlendMode::Multiply => {
            quote_expression!(image::blend_modes::blend_multiply(pattern_color, mask_color).r)
        }
        TextureBlendMode::Subtract => {
            quote_expression!(image::blend_modes::blend_subtract(pattern_color, mask_color).r)
        }
        TextureBlendMode::Darken => {
            quote_expression!(image::blend_modes::blend_darken(pattern_color, mask_color).r)
        }
        TextureBlendMode::Overlay => {
            quote_expression!(image::blend_modes::blend_overlay(pattern_color, mask_color).r)
        }
        TextureBlendMode::ColorDodge => {
            quote_expression!(image::blend_modes::blend_color_dodge(pattern_color, mask_color).r)
        }
        TextureBlendMode::ColorBurn => {
            quote_expression!(image::blend_modes::blend_color_burn(pattern_color, mask_color).r)
        }
        TextureBlendMode::LinearDodge => {
            quote_expression!(image::blend_modes::blend_linear_dodge(pattern_color, mask_color).r)
        }
        TextureBlendMode::LinearBurn => {
            quote_expression!(image::blend_modes::blend_linear_burn(pattern_color, mask_color).r)
        }
        TextureBlendMode::HardMix => {
            quote_expression!(image::blend_modes::blend_hard_mix(pattern_color, mask_color).r)
        }
        TextureBlendMode::Height => {
            quote_expression!(image::blend_modes::blend_height(pattern_color, mask_color).r)
        }
        TextureBlendMode::LinearHeight => {
            quote_expression!(image::blend_modes::blend_linear_height(pattern_color, mask_color).r)
        }
    };
    quote_statement! {{
        let pattern_sample = #pattern_sample;
        let pattern_color = vec4f(vec3f(#texture_value), 1.0);
        let mask_color = vec4f(vec3f(copy_mask), 1.0);
        let textured_mask = select(0.0, #blended_mask, copy_mask > 0.0);
        copy_mask = mix(copy_mask, textured_mask, #effective_depth);
    }}
}

fn tip_color_statement(
    flow: f32,
    flow_dynamics: Option<Dynamics>,
    opacity_dynamics: Option<Dynamics>,
    pose: BrushPose,
) -> Statement {
    let color = (*MAIN_COLOR_IDENT).clone();
    let effective_flow = match flow_dynamics {
        Some(dynamics) => {
            let factor = dynamics_factor(dynamics, 19.673, pose);
            quote_expression!(#flow * #factor)
        }
        None => quote_expression!(#flow * 1.0),
    };
    let Some(dynamics) = opacity_dynamics else {
        return quote_statement! {{
            let tip_color = vec4f(
                tip_foreground.rgb,
                tip_foreground.a * tip_mask * #effective_flow,
            );
            #color = image::blend_modes::blend_normal(
                tip_color,
                current_input_color(pixel_pos),
            );
        }};
    };
    let opacity_factor = dynamics_factor(dynamics, 39.346, pose);

    quote_statement! {{
        let tip_color = vec4f(
            tip_foreground.rgb,
            tip_foreground.a * tip_mask * #effective_flow,
        );
        let previous_color = current_input_color(pixel_pos);
        let accumulated_color = image::blend_modes::blend_normal(
            tip_color,
            previous_color,
        );
        #color = vec4f(
            accumulated_color.rgb,
            max(
                previous_color.a,
                min(accumulated_color.a, tip_mask * #opacity_factor),
            ),
        );
    }}
}

pub fn computed_required_spacing(
    diameter: f32,
    spacing: f32,
    size_dynamics: Option<Dynamics>,
    pose: BrushPose,
) -> String {
    let required_spacing = (*REQUIRED_SPACING_IDENT).clone();
    let size_diameter = size_diameter_statement(diameter, size_dynamics, pose);
    quote_statement! {{
        var tip_diameter: f32;
        @#size_diameter {}
        #required_spacing = max(tip_diameter * #spacing, 0.001);
    }}
    .to_string()
}

pub fn computed_main(
    diameter: f32,
    hardness: f32,
    angle: f32,
    roundness: f32,
    flip_x: bool,
    flip_y: bool,
    flow: f32,
    size_dynamics: Option<Dynamics>,
    opacity_dynamics: Option<Dynamics>,
    flow_dynamics: Option<Dynamics>,
    angle_dynamics: Option<Dynamics>,
    roundness_dynamics: Option<Dynamics>,
    tilt_scale: f32,
    pose: BrushPose,
    color_adjustment: Option<ColorAdjustment>,
    scatter: Option<Scatter>,
    brush_texture: Option<BrushTexture>,
) -> String {
    let pixel = (*MAIN_PIXEL_POSITION_IDENT).clone();
    let pen = (*MAIN_PEN_POSITION_IDENT).clone();
    let bounds = (*MAIN_BOUNDS_IDENT).clone();
    let flip_x = if flip_x { -1.0f32 } else { 1.0f32 };
    let flip_y = if flip_y { -1.0f32 } else { 1.0f32 };
    let size_diameter = size_diameter_statement(diameter, size_dynamics, pose);
    let tip_roundness = tip_roundness_statement(roundness, roundness_dynamics, tilt_scale, pose);
    let tip_angle = tip_angle_expression(angle, angle_dynamics, pose);
    let tip_foreground = tip_foreground_statement(color_adjustment, pose);
    let tip_color = tip_color_statement(flow, flow_dynamics, opacity_dynamics, pose);
    let scatter_offset = scatter_offset_expression(scatter, pose);
    let active_copy_count = scatter_count_expression(scatter, pose);
    let texture_mask = texture_mask_statement(brush_texture, pose, diameter, tip_angle.clone());

    quote_statement! {{
        var tip_diameter: f32;
        var tip_roundness: f32;
        var roundness_angle: f32;
        var tip_foreground: vec4f;
        @#size_diameter {}
        @#tip_roundness {}
        @#tip_foreground {}
        let tip_radius = tip_diameter * 0.5;
        let tip_radii = vec2f(
            tip_radius,
            max(tip_radius * tip_roundness, 0.001),
        );
        let tip_edge = max(
            1.0,
            max(tip_radii.x, tip_radii.y) * (1.0 - #hardness),
        );
        let active_copy_count = #active_copy_count;
        var tip_mask = 0.0;
        var combined_bounds: Rect;
        for (var copy_index = 0u; copy_index < active_copy_count; copy_index++) {
            let tip_center = #pen + #scatter_offset;
            let tip_delta = (#pixel - tip_center) * vec2f(#flip_x, #flip_y);
            let tip_distance = sdf_ellipse(
                rotate_mat2x2(#tip_angle + roundness_angle) * tip_delta,
                vec2f(0.0),
                tip_radii,
            );
            var copy_mask = smoothstep(tip_edge, -tip_edge, tip_distance);
            @#texture_mask {}
            tip_mask = 1.0 - (1.0 - tip_mask) * (1.0 - copy_mask);
            let copy_bounds = Rect(
                tip_center - vec2f(tip_radius),
                tip_center + vec2f(tip_radius),
            );
            if copy_index == 0u {
                combined_bounds = copy_bounds;
            } else {
                combined_bounds = Rect(
                    min(combined_bounds.min, copy_bounds.min),
                    max(combined_bounds.max, copy_bounds.max),
                );
            }
        }
        @#tip_color {}
        #bounds = combined_bounds;
    }}
    .to_string()
}

pub fn sampled_main(
    diameter: f32,
    angle: f32,
    roundness: f32,
    flip_x: bool,
    flip_y: bool,
    flow: f32,
    size_dynamics: Option<Dynamics>,
    opacity_dynamics: Option<Dynamics>,
    flow_dynamics: Option<Dynamics>,
    angle_dynamics: Option<Dynamics>,
    roundness_dynamics: Option<Dynamics>,
    tilt_scale: f32,
    pose: BrushPose,
    color_adjustment: Option<ColorAdjustment>,
    scatter: Option<Scatter>,
    brush_texture: Option<BrushTexture>,
) -> String {
    let texture = (*MAIN_TIP_TEXTURE_IDENT).clone();
    let pixel = (*MAIN_PIXEL_POSITION_IDENT).clone();
    let pen = (*MAIN_PEN_POSITION_IDENT).clone();
    let bounds = (*MAIN_BOUNDS_IDENT).clone();
    let flip_x = if flip_x { -1.0f32 } else { 1.0f32 };
    let flip_y = if flip_y { -1.0f32 } else { 1.0f32 };
    let size_diameter = size_diameter_statement(diameter, size_dynamics, pose);
    let tip_roundness = tip_roundness_statement(roundness, roundness_dynamics, tilt_scale, pose);
    let tip_angle = tip_angle_expression(angle, angle_dynamics, pose);
    let tip_foreground = tip_foreground_statement(color_adjustment, pose);
    let tip_color = tip_color_statement(flow, flow_dynamics, opacity_dynamics, pose);
    let scatter_offset = scatter_offset_expression(scatter, pose);
    let active_copy_count = scatter_count_expression(scatter, pose);
    let texture_mask = texture_mask_statement(brush_texture, pose, diameter, tip_angle.clone());

    quote_statement! {{
        var tip_diameter: f32;
        var tip_roundness: f32;
        var roundness_angle: f32;
        var tip_foreground: vec4f;
        @#size_diameter {}
        @#tip_roundness {}
        @#tip_foreground {}
        let tip_texture_size = vec2f(atlas_size(#texture));
        let tip_base_size = max(tip_texture_size.x, tip_texture_size.y);
        let tip_scale = vec2f(
            #flip_x * tip_diameter / tip_base_size,
            #flip_y * tip_diameter * tip_roundness / tip_base_size,
        );
        let tip_anchor = vec2f(0.5);
        let active_copy_count = #active_copy_count;
        var tip_mask = 0.0;
        var combined_bounds: Rect;
        for (var copy_index = 0u; copy_index < active_copy_count; copy_index++) {
            let tip_center = #pen + #scatter_offset;
            let tip_sample = sample_transformed_local_texture_clamp(
                #texture,
                #pixel,
                tip_scale,
                #tip_angle + roundness_angle,
                tip_center,
                tip_anchor,
            );
            var copy_mask = tip_sample.a * (1.0 - tip_sample.r);
            @#texture_mask {}
            tip_mask = 1.0 - (1.0 - tip_mask) * (1.0 - copy_mask);
            let copy_bounds = filter_within_mask_bounds(
                #texture,
                tip_scale,
                #tip_angle + roundness_angle,
                tip_center,
                tip_anchor,
            );
            if copy_index == 0u {
                combined_bounds = copy_bounds;
            } else {
                combined_bounds = Rect(
                    min(combined_bounds.min, copy_bounds.min),
                    max(combined_bounds.max, copy_bounds.max),
                );
            }
        }
        @#tip_color {}
        #bounds = combined_bounds;
    }}
    .to_string()
}

pub fn opacity_postprocess(opacity: f32) -> String {
    let input_color = (*POSTPROCESS_INPUT_COLOR_IDENT).clone();
    let stroke_bounds = (*POSTPROCESS_STROKE_BOUNDS_IDENT).clone();
    let color = (*MAIN_COLOR_IDENT).clone();
    let bounds = (*MAIN_BOUNDS_IDENT).clone();

    quote_statement! {{
        #color = vec4f(
            #input_color.rgb,
            #input_color.a * #opacity,
        );
        #bounds = #stroke_bounds;
    }}
    .to_string()
}
