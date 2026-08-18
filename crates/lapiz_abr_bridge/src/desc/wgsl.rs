use std::sync::LazyLock;

use wesl::syntax::*;
use wesl_quote::quote_statement;

pub const MAIN_PIXEL_POSITION_INPUT: &str = "pixel_position";
pub const MAIN_PEN_POSITION_INPUT: &str = "pen_position";
pub const MAIN_FOREGROUND_COLOR_INPUT: &str = "foreground_color";
pub const MAIN_TIP_TEXTURE_INPUT: &str = "tip_texture";
pub const MAIN_COLOR_OUTPUT: &str = "color";
pub const MAIN_BOUNDS_OUTPUT: &str = "bounds";
pub const POSTPROCESS_INPUT_COLOR: &str = "input_color";
pub const POSTPROCESS_STROKE_BOUNDS_INPUT: &str = "stroke_bounds";
pub const REQUIRED_SPACING_OUTPUT: &str = "required_spacing";

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

pub fn computed_required_spacing(diameter: f32, spacing: f32) -> String {
    let required_spacing = (*REQUIRED_SPACING_IDENT).clone();
    quote_statement! {
        #required_spacing = max(#diameter * #spacing, 0.001);
    }
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
) -> String {
    let pixel = (*MAIN_PIXEL_POSITION_IDENT).clone();
    let pen = (*MAIN_PEN_POSITION_IDENT).clone();
    let foreground = (*MAIN_FOREGROUND_COLOR_IDENT).clone();
    let color = (*MAIN_COLOR_IDENT).clone();
    let bounds = (*MAIN_BOUNDS_IDENT).clone();
    let flip_x = if flip_x { -1.0f32 } else { 1.0f32 };
    let flip_y = if flip_y { -1.0f32 } else { 1.0f32 };
    let radius = diameter * 0.5;

    quote_statement! {{
        let tip_delta = (#pixel - #pen) * vec2f(#flip_x, #flip_y);
        let tip_radii = vec2f(#radius, max(#radius * #roundness, 0.001));
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
        let tip_color = vec4f(
            #foreground.rgb,
            #foreground.a * tip_mask * #flow,
        );
        #color = image::blend_modes::blend_normal(
            tip_color,
            current_input_color(pixel_pos),
        );
        #bounds = Rect(
            #pen - vec2f(#radius),
            #pen + vec2f(#radius),
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
) -> String {
    let texture = (*MAIN_TIP_TEXTURE_IDENT).clone();
    let pixel = (*MAIN_PIXEL_POSITION_IDENT).clone();
    let pen = (*MAIN_PEN_POSITION_IDENT).clone();
    let foreground = (*MAIN_FOREGROUND_COLOR_IDENT).clone();
    let color = (*MAIN_COLOR_IDENT).clone();
    let bounds = (*MAIN_BOUNDS_IDENT).clone();
    let flip_x = if flip_x { -1.0f32 } else { 1.0f32 };
    let flip_y = if flip_y { -1.0f32 } else { 1.0f32 };

    quote_statement! {{
        let tip_texture_size = vec2f(atlas_size(#texture));
        let tip_base_size = max(tip_texture_size.x, tip_texture_size.y);
        let tip_scale = vec2f(
            #flip_x * #diameter / tip_base_size,
            #flip_y * #diameter * #roundness / tip_base_size,
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
        let tip_color = vec4f(
            #foreground.rgb,
            #foreground.a * tip_mask * #flow,
        );
        #color = image::blend_modes::blend_normal(
            tip_color,
            current_input_color(pixel_pos),
        );
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
