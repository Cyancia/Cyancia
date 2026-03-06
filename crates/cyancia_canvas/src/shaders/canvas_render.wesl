struct Canvas {
    transform: mat3x3f,
    inverse_transform: mat3x3f,
    size: vec2u,
    tile_count: vec2u,
    tile_size: u32,
}

// TODO: This is weird. This is a bindless array, and theoretically it won't have a fixed size.
//       It doesn't compile if remove the size, but works if it's set to a random value. Why? ｢( • ﾍ •)
@group(0) @binding(0) var tiles: binding_array<texture_storage_2d<r32uint, read>, 1>;
@group(0) @binding(1) var<storage> canvas: Canvas;
@group(0) @binding(2) var output: texture_storage_2d<rgba16float, write>;

@compute
@workgroup_size(16, 16, 1)
fn main(@builtin(global_invocation_id) index: vec3u) {
    let pixel_pos = index.xy;
    let canvas_pos = canvas.inverse_transform * vec3f(vec2f(pixel_pos), 1.0);
    if any(canvas_pos < vec3f(0.0)) || any(canvas_pos.xy >= vec2f(canvas.size)) {
        textureStore(output, vec2u(pixel_pos), vec4f(0.5, 0.5, 0.5, 1.0));
        return;
    }

    let tile_index = vec2u(canvas_pos.xy) / canvas.tile_size;
    if any(tile_index >= canvas.tile_count) {
        textureStore(output, vec2u(pixel_pos), vec4f(0.5, 0.5, 0.5, 1.0));
        return;
    }

    let flattened_index = tile_index.y * canvas.tile_count.x + tile_index.x;
    let tile_sample_coord = vec2u(canvas_pos.xy) % vec2u(canvas.tile_size);
    let color = image::texture_unpack::unpack_rgba8_texel(textureLoad(tiles[flattened_index], tile_sample_coord));

    textureStore(output, vec2u(pixel_pos), color);
    // textureStore(output, vec2u(pixel_pos), vec4f(tile_sample_coord, 0.0, 1.0));
}
