import image::image_tiling::{TileInfo, TILE_SIZE};
import image::texture_unpack::unpack_rgba8_texel;

@group(0) @binding(0) var result: texture_storage_2d_array<r32uint, read>;
@group(0) @binding(1) var<storage> result_tile_info: array<TileInfo>;
@group(0) @binding(2) var<storage> result_bounds: vec4i;
@group(0) @binding(3) var output: texture_storage_2d<rgba8unorm, write>;

fn load_result(pixel: vec2i) -> vec4f {
    for (var i = 0u; i < arrayLength(&result_tile_info); i += 1u) {
        let tile = result_tile_info[i];
        if all(pixel >= tile.origin) && all(pixel < tile.origin + i32(TILE_SIZE)) {
            return unpack_rgba8_texel(textureLoad(result, pixel - tile.origin, i));
        }
    }

    return vec4f(0.0);
}

fn sample_result(pixel: vec2f) -> vec4f {
    let base = vec2i(floor(pixel));
    let fraction = fract(pixel);
    let top = mix(load_result(base), load_result(base + vec2i(1, 0)), fraction.x);
    let bottom = mix(
        load_result(base + vec2i(0, 1)),
        load_result(base + vec2i(1, 1)),
        fraction.x,
    );
    return mix(top, bottom, fraction.y);
}

@compute
@workgroup_size(16, 16, 1)
fn main(@builtin(global_invocation_id) global_id: vec3u) {
    if any(global_id.xy >= textureDimensions(output)) {
        return;
    }

    let output_size = vec2f(textureDimensions(output));
    let bounds_min = vec2f(result_bounds.xy);
    let bounds_size = vec2f(result_bounds.zw - result_bounds.xy);

    if any(bounds_size == vec2f(0.0)) {
        return;
    }

    let scale = min(output_size.x / bounds_size.x, output_size.y / bounds_size.y);
    let scaled_bounds_size = bounds_size * scale;
    let scaled_bounds_min = (output_size - scaled_bounds_size) * 0.5;

    let pixel = vec2f(global_id.xy) + 0.5;
    let result_pixel = bounds_min + (pixel - scaled_bounds_min) / scale - 0.5;

    textureStore(output, global_id.xy, sample_result(result_pixel));
}
