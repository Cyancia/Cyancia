import package::types::BucketParams;
import image::image_tiling::{TileInfo, TILE_SIZE};

@group(0) @binding(0) var<uniform> params: BucketParams;
@group(0) @binding(1) var mask: texture_storage_2d_array<r8unorm, read>;
@group(0) @binding(2) var<storage> mask_tile_info: array<TileInfo>;
@group(0) @binding(3) var output: texture_storage_2d_array<r32uint, write>;
@group(0) @binding(4) var<storage> output_tile_info: array<TileInfo>;
@group(0) @binding(5) var dst: texture_storage_2d_array<r32uint, read>;
@group(0) @binding(6) var<storage> dst_tile_info: array<TileInfo>;
@group(0) @binding(7) var<storage> has_selection: u32;
@group(0) @binding(8) var selection: texture_storage_2d_array<r8unorm, read>;
@group(0) @binding(9) var<storage> selection_tile_info: array<TileInfo>;

fn find_dst_layer_index(tile_index: vec2i) -> i32 {
    for (var i = 0u; i < arrayLength(&dst_tile_info); i += 1u) {
        if all(dst_tile_info[i].index == tile_index) {
            return i32(i);
        }
    }
    return -1;
}

fn find_output_layer_index(tile_index: vec2i) -> i32 {
    for (var i = 0u; i < arrayLength(&output_tile_info); i += 1u) {
        if all(output_tile_info[i].index == tile_index) {
            return i32(i);
        }
    }
    return -1;
}

fn load_selection_value(pixel: vec2i) -> f32 {
    if has_selection == 0u {
        return 1.0;
    }

    for (var i = 0u; i < arrayLength(&selection_tile_info); i += 1u) {
        let tile = selection_tile_info[i];
        if all(pixel >= tile.origin) && all(pixel < tile.origin + i32(TILE_SIZE)) {
            return textureLoad(selection, pixel - tile.origin, i).r;
        }
    }
    return 0.0;
}

fn blend(src: vec4f, dst: vec4f) -> vec4f {
    //CODEGEN_FLAG_BLEND_FUNCTION
}

@compute
@workgroup_size(16, 16, 1)
fn main(@builtin(global_invocation_id) global_id: vec3u) {
    let src_tile_index = global_id.z;
    let local_index = global_id.xy;

    let global_index = mask_tile_info[src_tile_index].origin + vec2i(local_index);
    let mask_value = textureLoad(mask, local_index, src_tile_index).r;
    let src_color = mix(vec4f(0.0), params.fill_color, mask_value);

    let dst_layer_index = find_dst_layer_index(mask_tile_info[src_tile_index].index);
    if dst_layer_index == -1 {
        return;
    }
    let output_layer_index = find_output_layer_index(mask_tile_info[src_tile_index].index);
    if output_layer_index == -1 {
        return;
    }
    let dst_color = image::texture_unpack::unpack_rgba8_texel(textureLoad(dst, local_index, dst_layer_index));

    let selection_value = load_selection_value(global_index);
    let color = blend(vec4f(src_color.rgb, src_color.a * selection_value), dst_color);
    textureStore(output, local_index, output_layer_index, image::texture_unpack::pack_rgba8_texel(color));
}
