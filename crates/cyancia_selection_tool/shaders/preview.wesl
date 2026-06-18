struct Canvas {
    pixel_to_widget: mat3x3f,
    widget_min: vec2f,
    screen_size: vec2f,
    time: f32,
}

@group(0) @binding(0) var<uniform> canvas: Canvas;

@vertex
fn vertex(@location(0) position_ps: vec2f) -> @builtin(position) vec4f {
    let position_ws = canvas.pixel_to_widget * vec3f(position_ps, 1.0);
    let position_ss = position_ws.xy;
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
