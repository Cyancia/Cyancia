struct FullscreenVertexOutput {
    @builtin(position) position: vec4f,
    @location(0) uv: vec2f,
}

@vertex
fn vertex(@builtin(vertex_index) vertex_index: u32) -> FullscreenVertexOutput {
    var output: FullscreenVertexOutput;
    let t = vec2f(f32(vertex_index / 2u), f32(vertex_index % 2u));
    output.position = vec4f(vec2f(t * 4. - 1.), 0., 1.);
    output.uv = t * 2.;
    output.uv.y = 1.0 - output.uv.y;
    return output;
}
