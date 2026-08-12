import image::image_tiling::{TileInfo, TILE_SIZE};

@if(RGBA8)
@group(0) @binding(0) var src_texture: texture_storage_2d_array<r32uint, read>;
@if(A8)
@group(0) @binding(0) var src_texture: texture_storage_2d_array<r8unorm, read>;

@if(RGBA8)
@group(0) @binding(1) var dst_texture: texture_storage_2d_array<r32uint, write>;
@if(A8)
@group(0) @binding(1) var dst_texture: texture_storage_2d_array<r8unorm, write>;

@group(0) @binding(2) var<storage> src_tile_info: array<TileInfo>;
@group(0) @binding(3) var<storage> dst_tile_info: array<TileInfo>;

@compute
@workgroup_size(16, 16, 1)
fn main(@builtin(global_invocation_id) global_id: vec3u) {
    let src_tile_index = global_id.z;
    let local_index = global_id.xy;
    let src_info = src_tile_info[src_tile_index];

    for (var dst_tile_index = 0u; dst_tile_index < arrayLength(&dst_tile_info); dst_tile_index += 1u) {
        if all(dst_tile_info[dst_tile_index].index == src_info.index) {
            let texel = textureLoad(src_texture, local_index, src_tile_index);
            textureStore(dst_texture, local_index, dst_tile_index, texel);
            return;
        }
    }
}
