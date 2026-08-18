pub const MAIN_PIXEL_POSITION_INPUT: &str = "pixel_position";
pub const MAIN_PEN_POSITION_INPUT: &str = "pen_position";
pub const MAIN_FOREGROUND_COLOR_INPUT: &str = "foreground_color";
pub const MAIN_TIP_TEXTURE_INPUT: &str = "tip_texture";
pub const MAIN_COLOR_OUTPUT: &str = "color";
pub const MAIN_BOUNDS_OUTPUT: &str = "bounds";
pub const REQUIRED_SPACING_OUTPUT: &str = "required_spacing";

pub fn computed_required_spacing(diameter: f32, spacing: f32) -> String {
    format!("{REQUIRED_SPACING_OUTPUT} = max({diameter:.8} * {spacing:.8}, 0.001);\n")
}

pub fn computed_main(
    diameter: f32,
    hardness: f32,
    angle: f32,
    roundness: f32,
    flip_x: bool,
    flip_y: bool,
) -> String {
    let flip_x = if flip_x { "-1.0" } else { "1.0" };
    let flip_y = if flip_y { "-1.0" } else { "1.0" };
    let radius = diameter * 0.5;

    format!(
        r#"let tip_delta = ({pixel} - {pen}) * vec2f({flip_x}, {flip_y});
let tip_radii = vec2f({radius:.8}, max({radius:.8} * {roundness:.8}, 0.001));
let tip_distance = sdf_ellipse(
    rotate_mat2x2({angle:.8}) * tip_delta,
    vec2f(0.0),
    tip_radii,
);
let tip_edge = max(
    1.0,
    max(tip_radii.x, tip_radii.y) * (1.0 - {hardness:.8}),
);
let tip_mask = smoothstep(tip_edge, -tip_edge, tip_distance);
let tip_color = vec4f(
    {foreground}.rgb,
    {foreground}.a * tip_mask,
);
{color} = image::blend_modes::blend_normal(
    tip_color,
    current_input_color(pixel_pos),
);
{bounds} = Rect(
    {pen} - vec2f({radius:.8}),
    {pen} + vec2f({radius:.8}),
);
"#,
        pixel = MAIN_PIXEL_POSITION_INPUT,
        pen = MAIN_PEN_POSITION_INPUT,
        flip_x = flip_x,
        flip_y = flip_y,
        radius = radius,
        roundness = roundness,
        angle = angle,
        hardness = hardness,
        color = MAIN_COLOR_OUTPUT,
        foreground = MAIN_FOREGROUND_COLOR_INPUT,
        bounds = MAIN_BOUNDS_OUTPUT,
    )
}

pub fn sampled_main(
    diameter: f32,
    angle: f32,
    roundness: f32,
    flip_x: bool,
    flip_y: bool,
) -> String {
    let flip_x = if flip_x { "-1.0" } else { "1.0" };
    let flip_y = if flip_y { "-1.0" } else { "1.0" };

    format!(
        r#"let tip_texture_size = vec2f(atlas_size({texture}));
let tip_base_size = max(tip_texture_size.x, tip_texture_size.y);
let tip_scale = vec2f(
    {flip_x} * {diameter:.8} / tip_base_size,
    {flip_y} * {diameter:.8} * {roundness:.8} / tip_base_size,
);
let tip_anchor = vec2f(0.5);
let tip_sample = sample_transformed_local_texture_clamp(
    {texture},
    {pixel},
    tip_scale,
    {angle:.8},
    {pen},
    tip_anchor,
);
let tip_mask = tip_sample.a * (1.0 - tip_sample.r);
let tip_color = vec4f(
    {foreground}.rgb,
    {foreground}.a * tip_mask,
);
{color} = image::blend_modes::blend_normal(
    tip_color,
    current_input_color(pixel_pos),
);
{bounds} = filter_within_mask_bounds(
    {texture},
    tip_scale,
    {angle:.8},
    {pen},
    tip_anchor,
);
"#,
        texture = MAIN_TIP_TEXTURE_INPUT,
        pixel = MAIN_PIXEL_POSITION_INPUT,
        pen = MAIN_PEN_POSITION_INPUT,
        foreground = MAIN_FOREGROUND_COLOR_INPUT,
        color = MAIN_COLOR_OUTPUT,
        bounds = MAIN_BOUNDS_OUTPUT,
        flip_x = flip_x,
        flip_y = flip_y,
        diameter = diameter,
        roundness = roundness,
        angle = angle,
    )
}
