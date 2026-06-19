struct Canvas {
    pixel_to_widget: mat3x3f,
    widget_min: vec2f,
    screen_size: vec2f,
    time: f32,
}

@group(0) @binding(0) var<uniform> canvas: Canvas;
@group(0) @binding(1) var<storage> line_vertices_ps: array<vec2f>;

// 0,3---5
// |\   |
// | \  |
// |  \ |
// |   \|
// 2----1,4
const PERP_OFFSETS: array<f32, 6> = array<f32, 6>(-1.0, 1.0, -1.0, -1.0, 1.0, 1.0);
@vertex
fn vertex(@builtin(vertex_index) vertex_index: u32) -> @builtin(position) vec4f {
    let seg_index = vertex_index / 6u;
    let seg_start = line_vertices_ps[seg_index];
    let seg_end = line_vertices_ps[seg_index + 1];
    let seg_dir = normalize(seg_end - seg_start);
    let seg_dir_perp_half = vec2f(-seg_dir.y, seg_dir.x) * 0.5;
    let mesh = array<vec2f, 6>(
        seg_start,
        seg_end,
        seg_end,
        seg_start,
        seg_end,
        seg_start
    );

    let seg_vertex_index = vertex_index % 6u;
    let position_ps = mesh[seg_vertex_index];
    let position_ws = canvas.pixel_to_widget * vec3f(position_ps, 1.0);
    let position_ss = position_ws.xy + PERP_OFFSETS[seg_vertex_index] * seg_dir_perp_half;
    let normalized = position_ss / canvas.screen_size;
    let ndc = normalized * 2.0 - 1.0;
    return vec4f(vec2f(ndc.x, -ndc.y), 0.0, 1.0);
}

fn marching_ants(pixel: vec2f) -> vec4f {
    let w = (pixel.x + pixel.y) / 4.0 + canvas.time * 2.0;
    let v = select(0.0, 1.0, u32(w) % 2u != 0u);
    return vec4f(v, v, v, 1.0);
}

@fragment
fn fragment(@builtin(position) position: vec4f) -> @location(0) vec4f {
    return marching_ants(position.xy);
}
