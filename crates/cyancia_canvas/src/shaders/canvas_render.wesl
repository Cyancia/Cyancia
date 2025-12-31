import super::fullscreen_vertex::FullscreenVertexOutput;

struct Canvas {
    transform: mat3x3f,
    inverse_transform: mat3x3f,
    size: vec2u,
    tile_count: vec2u,
    tile_size: u32,
}

@group(0) @binding(0) var tile_pile: texture_2d_array<f32>;
@group(0) @binding(1) var tile_sampler: sampler;
@group(0) @binding(2) var<uniform> canvas: Canvas;
@group(0) @binding(3) var<storage, read> tile_mapper: array<u32>;
@group(0) @binding(4) var output: texture_storage_2d<rgba16float, write>;

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
    let tile_layer_id = tile_mapper[flattened_index];
    if tile_layer_id == 0xFFFFFFFF {
        return;
    }
    let tile_sample_coord = (canvas_pos.xy % vec2f(canvas.tile_size)) / vec2f(canvas.tile_size);
    let color = textureSampleLevel(tile_pile, tile_sampler, tile_sample_coord, tile_layer_id, 0.0);

    // textureStore(output, vec2u(pixel_pos), color);
    textureStore(output, vec2u(tile_sample_coord), color);
}
