import image::{
    blend_modes::blend_normal,
    image_tiling::{TileInfo, TILE_SIZE},
    texture_unpack::{pack_rgba8_texel, unpack_rgba8_texel},
};

@if(RGBA8)
@group(0) @binding(0) var layer: texture_storage_2d_array<r32uint, read>;
@if(RGBA8)
@group(0) @binding(1) var output: texture_storage_2d_array<r32uint, write>;
@if(A8)
@group(0) @binding(0) var layer: texture_storage_2d_array<r8unorm, read>;
@if(A8)
@group(0) @binding(1) var output: texture_storage_2d_array<r8unorm, write>;

@group(0) @binding(2) var<storage> layer_tile_info: array<TileInfo>;
@group(0) @binding(3) var<storage> output_tile_info: array<TileInfo>;
@group(0) @binding(4) var<storage> transform: Transform;

@if(WITH_SELECTION)
@group(0) @binding(5) var selection: texture_storage_2d_array<r8unorm, read>;
@if(WITH_SELECTION)
@group(0) @binding(6) var<storage> selection_tile_info: array<TileInfo>;

struct Transform {
    mat_inv: mat3x3f,
}

@if(RGBA8)
alias Pixel = vec4u;
@if(A8)
alias Pixel = vec4f;

@if(WITH_SELECTION)
fn sample_selection(position_ps: vec2i) -> f32 {
    for (var i = 0u; i < arrayLength(&selection_tile_info); i += 1u) {
        let tile = selection_tile_info[i];
        if all(position_ps >= tile.origin) && all(position_ps < tile.origin + i32(TILE_SIZE)) {
            return textureLoad(selection, position_ps - tile.origin, i).r;
        }
    }
    return 0.0;
}

@if(!WITH_SELECTION)
fn sample_selection(position_ps: vec2i) -> f32 {
    return 1.0;
}

fn load_layer(position_ps: vec2i) -> Pixel {
    for (var i = 0u; i < arrayLength(&layer_tile_info); i += 1u) {
        let tile = layer_tile_info[i];
        if all(position_ps >= tile.origin) && all(position_ps < tile.origin + i32(TILE_SIZE)) {
            return textureLoad(layer, position_ps - tile.origin, i);
        }
    }
    var transparent: Pixel;
    @if(RGBA8) { transparent = vec4<u32>(0u); }
    @if(A8) { transparent = vec4f(0.0); }
    return transparent;
}

fn sample_layer(position_ps: vec2f) -> Pixel {
    // TODO more filtering methods
    return load_layer(vec2i(position_ps));
}

fn to_vec4f(pixel: Pixel) -> vec4f {
    @if(RGBA8) { return unpack_rgba8_texel(pixel); }
    @if(A8) { return pixel; }
}

fn to_pixel(color: vec4f) -> Pixel {
    @if(RGBA8) { return pack_rgba8_texel(color); }
    @if(A8) { return vec4f(color.a, 0.0, 0.0, 0.0); }
}

@compute
@workgroup_size(16, 16, 1)
fn main_selection(@builtin(global_invocation_id) global_id: vec3u) {
    let output_tile_index = global_id.z;
    let output_local_index = global_id.xy;
    let position_ps = output_tile_info[output_tile_index].origin + vec2i(output_local_index);
    let positionf_ps = vec2f(position_ps);
    let original_ps = (transform.mat_inv * vec3f(positionf_ps, 1.0)).xy;
    textureStore(output, output_local_index, output_tile_index, sample_layer(original_ps));
}

@compute
@workgroup_size(16, 16, 1)
fn main(@builtin(global_invocation_id) global_id: vec3u) {
    let output_tile_index = global_id.z;
    let output_local_index = global_id.xy;
    let position_ps = output_tile_info[output_tile_index].origin + vec2i(output_local_index);
    let positionf_ps = vec2f(position_ps);
    let original_ps = (transform.mat_inv * vec3f(positionf_ps, 1.0)).xy;

    var result = to_vec4f(load_layer(position_ps));
    let cut = 1.0 - sample_selection(position_ps);
    result.a *= cut;

    let src_selection = sample_selection(vec2i(original_ps));
    let src = to_vec4f(sample_layer(original_ps));
    if src_selection > 0.0 && src.a > 0.0 {
        result = blend_normal(src, result);
    }

    textureStore(output, output_local_index, output_tile_index, to_pixel(result));
}
