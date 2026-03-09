import image::image_tiling::{TILE_SIZE, TileInfo};

struct Canvas {
    transform: mat3x3f,
    inverse_transform: mat3x3f,
    size: vec2u,
    tile_count: vec2u,
    tile_size: u32,
}

// TODO: This is weird. This is a bindless array, and theoretically it won't have a fixed size.
//       It doesn't compile if remove the size, but works if it's set to a random value. Why? ｢( • ﾍ •)
@group(0) @binding(0) var tiles: texture_storage_2d_array<r32uint, read>;
@group(0) @binding(1) var<storage> tile_info: array<TileInfo>;
@group(0) @binding(2) var<uniform> canvas: Canvas;
@group(0) @binding(3) var output: texture_storage_2d<rgba16float, write>;

const BACKGROUND_COLOR: vec4f = vec4f(0.5, 0.5, 0.5, 1.0);

fn checkerboard_pattern(pos: vec2u) -> vec4f {
    let checker_size = 8u;
    let checker_pos = pos / checker_size;
    let is_dark = (checker_pos.x + checker_pos.y) % 2u == 0u;
    return mix(vec4f(0.6, 0.6, 0.6, 1.0), vec4f(0.4, 0.4, 0.4, 1.0), f32(is_dark));
}

fn tile_checkerboard_pattern(pos: vec2u) -> vec4f {
    let checker_pos = pos / (TILE_SIZE);
    let is_dark = (checker_pos.x + checker_pos.y) % 2u == 0u;
    return mix(vec4f(1.0, 0.9, 0.9, 1.0), vec4f(0.9, 1.0, 0.9, 1.0), f32(is_dark));
}

fn convert_pixel_to_tile_index(pixel: vec2i) -> vec3u {
    for (var i = 0u; i < arrayLength(&tile_info); i = i + 1u) {
        let info = tile_info[i];
        if (pixel.x >= info.origin.x && pixel.x < info.origin.x + i32(TILE_SIZE) &&
            pixel.y >= info.origin.y && pixel.y < info.origin.y + i32(TILE_SIZE)) {
            return vec3u(vec2u(pixel - info.origin), i);
        }
    }
    return vec3u(0xffffffffu, 0, 0);
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

    let tile_index = convert_pixel_to_tile_index(vec2i(canvas_pos.xy));

    var color = vec4f(0.0);
    if tile_index.z != 0xffffffffu {
        color = image::texture_unpack::unpack_rgba8_texel(textureLoad(tiles, tile_index.xy, tile_index.z));
    }

    textureStore(output, vec2u(pixel_pos), mix(checkerboard_pattern(vec2u(canvas_pos.xy)), color, color.a) * tile_checkerboard_pattern(vec2u(canvas_pos.xy)));
}
