import image::image_tiling::{TILE_SIZE, TileInfo};

@group(0) @binding(0) var<storage> mask: array<f32>;
@group(0) @binding(1) var output: texture_storage_2d_array<rgba8unorm, write>;

@compute
@workgroup_size(16, 16, 1)
fn main(@builtin(global_invocation_id) global_id: vec3u) {
    let tile_index = global_id.z;
    let pixel_index = vec2u(global_id.x, global_id.y);

    let linear_index = tile_index * TILE_SIZE * TILE_SIZE + pixel_index.y * TILE_SIZE + pixel_index.x;
    textureStore(output, pixel_index, tile_index, vec4f(mask[linear_index]));
}
