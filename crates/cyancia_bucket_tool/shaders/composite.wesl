import package::types::BucketParams;
import image::image_tiling::{TileInfo, TILE_SIZE};

@group(0) @binding(0) var<uniform> params: BucketParams;
@group(0) @binding(1) var mask: texture_storage_2d_array<r32uint, read>;
@group(0) @binding(2) var output: texture_storage_2d_array<r32uint, read_write>;

@compute
@workgroup_size(16, 16, 1)
fn main(@builtin(global_invocation_id) global_id: vec3u) {
    let tile_index = global_id.z;
    let pixel_index = global_id.xy;
    let mask_value = bitcast<f32>(textureLoad(mask, pixel_index, tile_index).r);
    let src_color = mix(vec4f(0.0), params.fill_color, mask_value);
    let dst_color = image::texture_unpack::unpack_rgba8_texel(textureLoad(output, pixel_index, tile_index));
    let color = image::blend_modes::blend_normal(src_color, dst_color);
    textureStore(output, pixel_index, tile_index, image::texture_unpack::pack_rgba8_texel(color));
}
