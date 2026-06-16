import package::types::{BucketParams, INVALID_LABEL};
import image::image_tiling::{TileInfo, TILE_SIZE};

@group(0) @binding(0) var<uniform> params: BucketParams;
@group(0) @binding(1) var input: texture_storage_2d_array<r32uint, read>;
@group(0) @binding(2) var<storage, read_write> mask: array<atomic<u32>>;
@group(0) @binding(3) var<storage> input_tile_info: array<TileInfo>;
@group(0) @binding(4) var<storage> mask_tile_info: array<TileInfo>;

fn store_mask(tile_idx: u32, local_coord: vec2u, value: f32) {
    let linear = tile_idx * TILE_SIZE * TILE_SIZE + local_coord.y * TILE_SIZE + local_coord.x;
    let index = linear / 4u;
    let offset = (linear % 4u) * 8u;
    let value_u32 = u32(value * 255.0) & 0xFFu;
    let byte_mask = 0xFFu << offset;
    atomicAnd(&mask[index], ~byte_mask);
    atomicOr(&mask[index], value_u32 << offset);
}

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
        store_mask(tile_index, pixel_index, 0.0);
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

    store_mask(tile_index, pixel_index, value);
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
