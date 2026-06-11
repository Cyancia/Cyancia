import package::types::BucketParams;
import image::image_tiling::{TileInfo, TILE_SIZE};

@group(0) @binding(0) var<uniform> params: BucketParams;
@group(0) @binding(1) var<storage> bit_mask: array<u32>;
@group(0) @binding(2) var output: texture_storage_2d_array<r32uint, read_write>;

@compute
@workgroup_size(16, 16, 1)
fn main(@builtin(global_invocation_id) global_id: vec3u) {
    let tile_index = global_id.z;
    let pixel_index = global_id.xy;
    let is_opaque = get_bit(tile_index, pixel_index);
    let src_color = select(vec4f(0.0), params.fill_color, is_opaque);
    let dst_color = image::texture_unpack::unpack_rgba8_texel(textureLoad(output, pixel_index, tile_index));
    let color = image::blend_modes::blend_normal(src_color, dst_color);
    textureStore(output, pixel_index, tile_index, image::texture_unpack::pack_rgba8_texel(color));
}

fn get_bit(tile_idx: u32, local_coord: vec2u) -> bool {
    let bit_index = tile_idx * TILE_SIZE * TILE_SIZE + local_coord.y * TILE_SIZE + local_coord.x;
    let array_index = bit_index / 32u;
    let bit_offset = bit_index % 32u;
    return (bit_mask[array_index] & (1u << bit_offset)) != 0u;
}
