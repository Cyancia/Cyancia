use std::sync::LazyLock;

use lapiz_abr::DynamicsControl;
use wesl::syntax::*;
use wesl_quote::{quote_expression, quote_statement};

pub const MAIN_PIXEL_POSITION_INPUT: &str = "pixel_position";
pub const MAIN_PEN_POSITION_INPUT: &str = "pen_position";
pub const MAIN_FOREGROUND_COLOR_INPUT: &str = "foreground_color";
pub const MAIN_TIP_TEXTURE_INPUT: &str = "tip_texture";
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

#[derive(Clone, Copy)]
pub struct Dynamics {
    pub control: DynamicsControl,
    pub fade_steps: i32,
    pub jitter: f32,
    pub minimum: f32,
}

// TODO: This is really ugly, can we avoid this?
static MAIN_PIXEL_POSITION_IDENT: LazyLock<Ident> =
    LazyLock::new(|| Ident::new("pixel_position".to_string()));
static MAIN_PEN_POSITION_IDENT: LazyLock<Ident> =
    LazyLock::new(|| Ident::new(MAIN_PEN_POSITION_INPUT.to_string()));
static MAIN_FOREGROUND_COLOR_IDENT: LazyLock<Ident> =
    LazyLock::new(|| Ident::new(MAIN_FOREGROUND_COLOR_INPUT.to_string()));
static MAIN_TIP_TEXTURE_IDENT: LazyLock<Ident> =
    LazyLock::new(|| Ident::new(MAIN_TIP_TEXTURE_INPUT.to_string()));
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

fn dynamics_factor(dynamics: Dynamics, random_offset: f32) -> Expression {
    let pressure = (*PRESSURE_IDENT).clone();
    let tilt = (*TILT_IDENT).clone();
    let azimuth = (*AZIMUTH_IDENT).clone();
    let direction = (*DIRECTION_IDENT).clone();
    let initial_direction = (*INITIAL_DIRECTION_IDENT).clone();
    let dab_index = (*DAB_INDEX_IDENT).clone();
    let control = match dynamics.control {
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
    };
    let jitter = dynamics.jitter;
    let minimum = dynamics.minimum;

    quote_expression!(mix(
        mix(#minimum, 1.0, #control),
        #minimum,
        #jitter
            * fract(
                sin(f32(#dab_index) * 12.9898 + #random_offset)
                    * 43758.5453
            ),
    ))
}

fn size_diameter_statement(diameter: f32, dynamics: Option<Dynamics>) -> Statement {
    let Some(dynamics) = dynamics else {
        return quote_statement! {
            tip_diameter = #diameter;
        };
    };
    let factor = dynamics_factor(dynamics, 78.233);

    quote_statement! {
        tip_diameter = #diameter * #factor;
    }
}

fn tip_color_statement(flow: f32, opacity_dynamics: Option<Dynamics>) -> Statement {
    let foreground = (*MAIN_FOREGROUND_COLOR_IDENT).clone();
    let color = (*MAIN_COLOR_IDENT).clone();
    let Some(dynamics) = opacity_dynamics else {
        return quote_statement! {{
            let tip_color = vec4f(
                #foreground.rgb,
                #foreground.a * tip_mask * #flow,
            );
            #color = image::blend_modes::blend_normal(
                tip_color,
                current_input_color(pixel_pos),
            );
        }};
    };
    let opacity_factor = dynamics_factor(dynamics, 39.346);

    quote_statement! {{
        let tip_color = vec4f(
            #foreground.rgb,
            #foreground.a * tip_mask * #flow,
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
) -> String {
    let required_spacing = (*REQUIRED_SPACING_IDENT).clone();
    let size_diameter = size_diameter_statement(diameter, size_dynamics);
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
) -> String {
    let pixel = (*MAIN_PIXEL_POSITION_IDENT).clone();
    let pen = (*MAIN_PEN_POSITION_IDENT).clone();
    let bounds = (*MAIN_BOUNDS_IDENT).clone();
    let flip_x = if flip_x { -1.0f32 } else { 1.0f32 };
    let flip_y = if flip_y { -1.0f32 } else { 1.0f32 };
    let size_diameter = size_diameter_statement(diameter, size_dynamics);
    let tip_color = tip_color_statement(flow, opacity_dynamics);

    quote_statement! {{
        var tip_diameter: f32;
        @#size_diameter {}
        let tip_radius = tip_diameter * 0.5;
        let tip_delta = (#pixel - #pen) * vec2f(#flip_x, #flip_y);
        let tip_radii = vec2f(tip_radius, max(tip_radius * #roundness, 0.001));
        let tip_distance = sdf_ellipse(
            rotate_mat2x2(#angle) * tip_delta,
            vec2f(0.0),
            tip_radii,
        );
        let tip_edge = max(
            1.0,
            max(tip_radii.x, tip_radii.y) * (1.0 - #hardness),
        );
        let tip_mask = smoothstep(tip_edge, -tip_edge, tip_distance);
        @#tip_color {}
        #bounds = Rect(
            #pen - vec2f(tip_radius),
            #pen + vec2f(tip_radius),
        );
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
) -> String {
    let texture = (*MAIN_TIP_TEXTURE_IDENT).clone();
    let pixel = (*MAIN_PIXEL_POSITION_IDENT).clone();
    let pen = (*MAIN_PEN_POSITION_IDENT).clone();
    let bounds = (*MAIN_BOUNDS_IDENT).clone();
    let flip_x = if flip_x { -1.0f32 } else { 1.0f32 };
    let flip_y = if flip_y { -1.0f32 } else { 1.0f32 };
    let size_diameter = size_diameter_statement(diameter, size_dynamics);
    let tip_color = tip_color_statement(flow, opacity_dynamics);

    quote_statement! {{
        var tip_diameter: f32;
        @#size_diameter {}
        let tip_texture_size = vec2f(atlas_size(#texture));
        let tip_base_size = max(tip_texture_size.x, tip_texture_size.y);
        let tip_scale = vec2f(
            #flip_x * tip_diameter / tip_base_size,
            #flip_y * tip_diameter * #roundness / tip_base_size,
        );
        let tip_anchor = vec2f(0.5);
        let tip_sample = sample_transformed_local_texture_clamp(
            #texture,
            #pixel,
            tip_scale,
            #angle,
            #pen,
            tip_anchor,
        );
        let tip_mask = tip_sample.a * (1.0 - tip_sample.r);
        @#tip_color {}
        #bounds = filter_within_mask_bounds(
            #texture,
            tip_scale,
            #angle,
            #pen,
            tip_anchor,
        );
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
