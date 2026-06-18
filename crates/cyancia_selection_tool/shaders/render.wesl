import image::image_tiling::{TileInfo, TILE_SIZE};

@group(0) @binding(0) var<uniform> cur_tile_index: vec2i;

@vertex
fn vertex(@location(0) position_ps: vec2f) -> @builtin(position) vec4f {
    let tile_min = cur_tile_index * i32(TILE_SIZE);

    let local_coord = position_ps - vec2f(tile_min);
    let t = local_coord / vec2f(f32(TILE_SIZE));
    let ndc = t * 2.0 - 1.0;
    return vec4f(vec2f(ndc.x, -ndc.y), 0.0, 1.0);
}

@fragment
fn fragment(@builtin(position) position_cs: vec4f) -> @location(0) vec4f {
    return vec4f(1.0, 0.0, 0.0, 0.0);
}
