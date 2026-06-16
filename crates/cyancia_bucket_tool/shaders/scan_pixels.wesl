import image::image_tiling::{TileInfo, TILE_SIZE};

@group(0) @binding(0) var<storage> mask: array<u32>;
@group(0) @binding(1) var<storage> mask_tile_info: array<TileInfo>;
@group(0) @binding(2) var<storage, read_write> is_not_empty: array<u32>;

fn load_mask(tile_idx: u32, local_coord: vec2u) -> f32 {
    let linear = tile_idx * TILE_SIZE * TILE_SIZE + local_coord.y * TILE_SIZE + local_coord.x;
    let index = linear / 4u;
    let offset = (linear % 4u) * 8u;
    let value = (mask[index] >> offset) & 0xFFu;
    return f32(value) / 255.0;
}

@compute
@workgroup_size(16, 16, 1)
fn main(@builtin(global_invocation_id) global_id: vec3u) {
    let tile_index = global_id.z;
    let local_index = global_id.xy;
    let local_mask_tile_info = mask_tile_info[tile_index];

    let mask_value = load_mask(tile_index, local_index);
    if mask_value > 0.0 {
        is_not_empty[tile_index] = 1u;
    }
}
