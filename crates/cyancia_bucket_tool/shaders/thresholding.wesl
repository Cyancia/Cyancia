import package::types::{BucketParams, INVALID_LABEL};
import image::image_tiling::{TileInfo, TILE_SIZE};

@group(0) @binding(0) var<uniform> params: BucketParams;
@group(0) @binding(1) var input: texture_storage_2d_array<r32uint, read>;
@group(0) @binding(2) var<storage, read_write> labels: array<u32>;
@group(0) @binding(3) var<storage> input_tile_info: array<TileInfo>;

@compute
@workgroup_size(16, 16, 1)
fn main(@builtin(global_invocation_id) global_id: vec3u) {
    let tile_index = global_id.z;
    let pixel_index = global_id.xy;
    if any(pixel_index >= vec2u(TILE_SIZE)) {
        return;
    }

    let seed_color = get_seed_color();
    let current_color = image::texture_unpack::unpack_rgba8_texel(textureLoad(input, pixel_index, tile_index));

    var matches = false;
    if seed_color.a >= params.alpha_threshold {
        if distance(seed_color.rgb, current_color.rgb) <= params.threshold {
            matches = true;
        }
    } else {
        if current_color.a <= params.alpha_threshold {
            matches = true;
        }
    }

    let linear_idx = tile_index * TILE_SIZE * TILE_SIZE + pixel_index.y * TILE_SIZE + pixel_index.x;
    let value = select(INVALID_LABEL, linear_idx, matches);
    labels[linear_idx] = value;
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
