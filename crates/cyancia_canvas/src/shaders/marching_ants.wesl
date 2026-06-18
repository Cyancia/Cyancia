import image::image_tiling::{TileInfo, TILE_SIZE};
import render::fullscreen_vertex::FullscreenVertexOutput;

struct MarchingAntParams {
    selection_inv_transform: mat3x3f,
    thickness: i32,
    t: f32,
}

@group(0) @binding(0) var selection: texture_storage_2d_array<r8unorm, read>;
@group(0) @binding(1) var selection_tile_info: array<TileInfo>;
@group(0) @binding(2) var<uniform> params: MarchingAntParams;

fn sample_selection_binary(pixel: vec2i) -> bool {
    for (var i = 0u; i < arrayLength(&selection_tile_info); i += 1u) {
        let tile = selection_tile_info[i];
        if all(pixel >= tile.origin) && all(pixel < tile.origin + i32(TILE_SIZE)) {
            let local = pixel - tile.origin;
            let value = textureLoad(selection, local, i).r;
            return value != 0.0;
        }
    }

    return false;
}

fn is_at_edge(pixel: vec2i) -> bool {
    let center = sample_selection_binary(pixel);
    for (var dx = -params.thickness; dx <= params.thickness; dx += 1) {
        for (var dy = -params.thickness; dy <= params.thickness; dy += 1) {
            if sample_selection_binary(pixel + vec2i(dx, dy)) != center {
                return true;
            }
        }
    }
    return false;
}

@fragment
fn fragment(in: FullscreenVertexOutput) -> @location(0) vec4f {
    let p = params.selection_inv_transform * vec3f(in.position.xy, 1.0);
    let w = p.x + p.y + params.t;
    let v = select(0.0, 1.0, u32(w) % 8u != 0u);
    return vec4f(v, v, v, 1.0);
}
