import package::types::BucketParams;
import image::image_tiling::{TileInfo, TILE_SIZE};

@group(0) @binding(0) var<uniform> params: BucketParams;
@group(0) @binding(1) var input: texture_storage_2d_array<r32uint, read>;
@group(0) @binding(2) var<storage, read_write> output: array<u32>;
@group(0) @binding(3) var<storage> input_tile_info: array<TileInfo>;

@compute
@workgroup_size(1, 1, 1)
fn main() {
    let seed_alpha = get_seed_alpha();
    output[0] = select(0u, 1u, seed_alpha <= params.alpha_threshold);
}

fn get_seed_alpha() -> f32 {
    let seed = vec2i(params.seed);
    for (var i = 0u; i < arrayLength(&input_tile_info); i = i + 1u) {
        let info = input_tile_info[i];
        if seed.x >= info.origin.x && seed.x < info.origin.x + i32(TILE_SIZE) &&
            seed.y >= info.origin.y && seed.y < info.origin.y + i32(TILE_SIZE) {
            return image::texture_unpack::unpack_rgba8_texel(textureLoad(input, seed - info.origin, i)).a;
        }
    }
    return 0.0;
}
