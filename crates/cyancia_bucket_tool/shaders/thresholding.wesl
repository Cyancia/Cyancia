import package::types::{BucketParams, INVALID_LABEL};
import image::image_tiling::{TileInfo, TILE_SIZE};

@group(0) @binding(0) var<uniform> params: BucketParams;
@group(0) @binding(1) var input: texture_storage_2d_array<r32uint, read>;
@group(0) @binding(2) var mask: texture_storage_2d_array<r8unorm, write>;
@group(0) @binding(3) var<storage> input_tile_info: array<TileInfo>;
@group(0) @binding(4) var<storage> mask_tile_info: array<TileInfo>;

@compute
@workgroup_size(16, 16, 1)
fn main(@builtin(global_invocation_id) global_id: vec3u) {
    let tile_index = global_id.z;
    let pixel_index = global_id.xy;
    if tile_index >= arrayLength(&mask_tile_info) || any(pixel_index >= vec2u(TILE_SIZE)) {
        return;
    }

    let mask_info = mask_tile_info[tile_index];
    let global_coord = mask_info.origin + vec2i(pixel_index);
    if is_outside_image(global_coord) {
        textureStore(mask, pixel_index, tile_index, vec4f(0.0));
        return;
    }

    let current_color = load_input_color(global_coord);

    var value = 0.0;
    if params.transparent_mode == 0u {
        let seed_color = get_seed_color();
        if distance(seed_color.rgb, current_color.rgb) <= params.threshold {
            value = 1.0;
        }
    } else {
        if current_color.a <= params.alpha_threshold {
            value = 1.0;
        }
    }

    textureStore(mask, pixel_index, tile_index, vec4f(value, 0.0, 0.0, 0.0));
}

fn is_outside_image(coord: vec2i) -> bool {
    return params.transparent_mode != 0u &&
        (coord.x < 0 || coord.y < 0 ||
         coord.x >= i32(params.image_size.x) || coord.y >= i32(params.image_size.y));
}

fn load_input_color(coord: vec2i) -> vec4f {
    for (var i = 0u; i < arrayLength(&input_tile_info); i = i + 1u) {
        let info = input_tile_info[i];
        if coord.x >= info.origin.x && coord.x < info.origin.x + i32(TILE_SIZE) &&
            coord.y >= info.origin.y && coord.y < info.origin.y + i32(TILE_SIZE) {
            return image::texture_unpack::unpack_rgba8_texel(textureLoad(input, coord - info.origin, i));
        }
    }
    return vec4f(0.0);
}

fn get_seed_color() -> vec4f {
    let seed = vec2i(params.seed);
    for (var i = 0u; i < arrayLength(&input_tile_info); i = i + 1u) {
        let info = input_tile_info[i];
        if seed.x >= info.origin.x && seed.x < info.origin.x + i32(TILE_SIZE) &&
            seed.y >= info.origin.y && seed.y < info.origin.y + i32(TILE_SIZE) {
            return image::texture_unpack::unpack_rgba8_texel(textureLoad(input, seed - info.origin, i));
        }
    }
    return vec4f(0.0);
}
