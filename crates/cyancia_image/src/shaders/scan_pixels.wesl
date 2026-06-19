import package::image_tiling::{TileInfo, TILE_SIZE};

@group(0) @binding(0) var mask: texture_storage_2d_array<r8unorm, read>;
@group(0) @binding(1) var<storage> mask_tile_info: array<TileInfo>;
@group(0) @binding(2) var<storage, read_write> is_not_empty: array<u32>;

@compute
@workgroup_size(16, 16, 1)
fn main(@builtin(global_invocation_id) global_id: vec3u) {
    let tile_index = global_id.z;
    let local_index = global_id.xy;
    let local_mask_tile_info = mask_tile_info[tile_index];

    let mask_value = textureLoad(mask, local_index, tile_index).r;
    if mask_value > 0.0 {
        is_not_empty[tile_index] = 1u;
    }
}

@compute
@workgroup_size(16, 16, 1)
fn scan_to_binary_buffer(@builtin(global_invocation_id) global_id: vec3u) {
    let tile_index = global_id.z;
    let local_index = global_id.xy;
    let local_mask_tile_info = mask_tile_info[tile_index];

    let mask_value = textureLoad(mask, local_index, tile_index).r;
    if mask_value > 0.0 {
        is_not_empty[0] = 1u;
    }
}
