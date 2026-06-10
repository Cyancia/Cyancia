import package::types::BucketParams;
import image::image_tiling::{TileInfo, TILE_SIZE};

@group(0) @binding(0) var<uniform> params: BucketParams;
@group(0) @binding(1) var input: texture_storage_2d_array<r32uint, read>;
@group(0) @binding(2) var<storage, read_write> output: array<atomic<u32>>;
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
    // We don't really care where the pixel actually is, so no need to iterate through the tiles
    // to find out which tile actually is.
    let current_color = image::texture_unpack::unpack_rgba8_texel(textureLoad(input, pixel_index, tile_index));

    if seed_color.a >= params.alpha_threshold {
        // Seed is opaque
        if distance(seed_color.rgb, current_color.rgb) <= params.threshold {
            // Opaque pixels are only connectable with opaque pixels whose euclidean distance is below the threshold,
            // ignoring alpha.
            set_bit(tile_index, pixel_index);
        }
    } else {
        // Seed is transparent
        if current_color.a <= params.alpha_threshold {
            // Transparent pixels are only connectable with transparent pixels, ignoring rgb.
            set_bit(tile_index, pixel_index);
        }
    }
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

fn set_bit(tile_index: u32, pixel_index: vec2u) {
    let bit_index = tile_index * TILE_SIZE * TILE_SIZE + pixel_index.y * TILE_SIZE + pixel_index.x;
    let index = bit_index / 32u;
    let bit_offset = bit_index % 32u;
    let mask = 1u << bit_offset;
    atomicOr(&output[index], mask);
}
