import image::image_tiling::{TILE_SIZE, TileInfo};

@group(0) @binding(0) var<storage> bit_mask: array<u32>;
@group(0) @binding(1) var output: texture_storage_2d_array<rgba8unorm, write>;

@compute
@workgroup_size(16, 16, 1)
fn main(@builtin(global_invocation_id) global_id: vec3u) {
    let tile_index = global_id.z;
    let pixel_index = vec2u(global_id.x, global_id.y);

    let color = select(vec4f(0.0), vec4f(1.0), get_bit(tile_index, pixel_index));
    textureStore(output, global_id.xy, global_id.z, color);
}

fn get_bit(tile_index: u32, pixel_index: vec2u) -> bool {
    let bit_index = tile_index * TILE_SIZE * TILE_SIZE + pixel_index.y * TILE_SIZE + pixel_index.x;
    let index = bit_index / 32u;
    let bit_offset = bit_index % 32u;
    let mask = 1u << bit_offset;
    let value = bit_mask[index];
    return (value & mask) != 0u;
}
