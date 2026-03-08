struct Canvas {
    transform: mat3x3f,
    inverse_transform: mat3x3f,
    size: vec2u,
    tile_count: vec2u,
    tile_size: u32,
}

// TODO: This is weird. This is a bindless array, and theoretically it won't have a fixed size.
//       It doesn't compile if remove the size, but works if it's set to a random value. Why? ｢( • ﾍ •)
@group(0) @binding(0) var tiles: binding_array<texture_storage_2d<r32uint, read>, 1>;
@group(0) @binding(1) var<storage> canvas: Canvas;
@group(0) @binding(2) var output: texture_storage_2d<rgba16float, write>;

const BACKGROUND_COLOR: vec4f = vec4f(0.5, 0.5, 0.5, 1.0);

fn checkerboard_pattern(pos: vec2u) -> vec4f {
    let checker_size = 8u;
    let checker_pos = pos / checker_size;
    let is_dark = (checker_pos.x + checker_pos.y) % 2u == 0u;
    return mix(vec4f(0.6, 0.6, 0.6, 1.0), vec4f(0.4, 0.4, 0.4, 1.0), f32(is_dark));
}

fn tile_checkerboard_pattern(pos: vec2u) -> vec4f {
    let checker_pos = pos / canvas.tile_size;
    let is_dark = (checker_pos.x + checker_pos.y) % 2u == 0u;
    return mix(vec4f(1.0, 0.9, 0.9, 1.0), vec4f(0.9, 1.0, 0.9, 1.0), f32(is_dark));
}

@compute
@workgroup_size(16, 16, 1)
fn main(@builtin(global_invocation_id) index: vec3u) {
    let pixel_pos = index.xy;
    let canvas_pos = canvas.inverse_transform * vec3f(vec2f(pixel_pos), 1.0);
    if any(canvas_pos < vec3f(0.0)) || any(canvas_pos.xy >= vec2f(canvas.size)) {
        textureStore(output, vec2u(pixel_pos), BACKGROUND_COLOR);
        return;
    }

    let tile_index = vec2u(canvas_pos.xy) / canvas.tile_size;
    if any(tile_index >= canvas.tile_count) {
        textureStore(output, vec2u(pixel_pos), BACKGROUND_COLOR);
        return;
    }

    let flattened_index = tile_index.y * canvas.tile_count.x + tile_index.x;
    let tile_sample_coord = vec2u(canvas_pos.xy) % vec2u(canvas.tile_size);
    let color = image::texture_unpack::unpack_rgba8_texel(textureLoad(tiles[flattened_index], tile_sample_coord));

    textureStore(output, vec2u(pixel_pos), mix(checkerboard_pattern(vec2u(canvas_pos.xy)), color, color.a) * tile_checkerboard_pattern(vec2u(canvas_pos.xy)));
    // textureStore(output, vec2u(pixel_pos), vec4f(tile_sample_coord, 0.0, 1.0));
}
