import render::fullscreen_vertex::FullscreenVertexOutput;

@group(0) @binding(0) var input_texture: texture_2d<f32>;
@group(0) @binding(1) var input_sampler: sampler;

@fragment
fn fragment(in: FullscreenVertexOutput) -> @location(0) vec4f {
    return textureSample(input_texture, input_sampler, in.uv);
}
