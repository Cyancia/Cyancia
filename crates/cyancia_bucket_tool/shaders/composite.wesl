import package::types::BucketParams;
import image::image_tiling::{TileInfo, TILE_SIZE};

@group(0) @binding(0) var<uniform> params: BucketParams;
@group(0) @binding(1) var<storage> mask: array<u32>;
@group(0) @binding(2) var<storage> mask_tile_info: array<TileInfo>;
@group(0) @binding(3) var output: texture_storage_2d_array<r32uint, read_write>;
@group(0) @binding(4) var<storage> output_tile_info: array<TileInfo>;

fn find_dst_layer_index(tile_index: vec2i) -> i32 {
    for (var i = 0u; i < arrayLength(&output_tile_info); i += 1u) {
        if all(output_tile_info[i].index == tile_index) {
            return i32(i);
        }
    }
    return -1;
}

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
    let src_tile_index = global_id.z;
    let local_index = global_id.xy;

    let global_index = mask_tile_info[src_tile_index].origin + vec2i(local_index);
    let mask_value = load_mask(src_tile_index, local_index);
    let src_color = mix(vec4f(0.0), params.fill_color, mask_value);

    let dst_layer_index = find_dst_layer_index(mask_tile_info[src_tile_index].index);
    if dst_layer_index == -1 {
        return;
    }
    let dst_color = image::texture_unpack::unpack_rgba8_texel(textureLoad(output, local_index, dst_layer_index));

    let color = image::blend_modes::blend_normal(src_color, dst_color);
    textureStore(output, local_index, dst_layer_index, image::texture_unpack::pack_rgba8_texel(color));
}
