import image::{
    blend_modes::blend_normal,
    image_tiling::{TileInfo, TILE_SIZE},
    texture_unpack::{pack_rgba8_texel, unpack_rgba8_texel},
};

@if(RGBA8) @group(0) @binding(0) var layer: texture_storage_2d_array<r32uint, read>;
@if(RGBA8) @group(0) @binding(1) var output: texture_storage_2d_array<r32uint, write>;
@if(A8) @group(0) @binding(0) var layer: texture_storage_2d_array<r8unorm, read>;
@if(A8) @group(0) @binding(1) var output: texture_storage_2d_array<r8unorm, write>;

@group(0) @binding(2) var<storage> layer_tile_info: array<TileInfo>;
@group(0) @binding(3) var<storage> output_tile_info: array<TileInfo>;
@group(0) @binding(4) var disp: texture_storage_2d_array<r32uint, read>;
@group(0) @binding(5) var<storage> disp_tile_info: array<TileInfo>;
@group(0) @binding(6) var<storage> has_selection: u32;
@group(0) @binding(7) var selection: texture_storage_2d_array<r8unorm, read>;
@group(0) @binding(8) var<storage> selection_tile_info: array<TileInfo>;

fn load_disp_texel(pos: vec2i) -> vec2f {
    for (var i = 0u; i < arrayLength(&disp_tile_info); i += 1u) {
        let tile = disp_tile_info[i];
        if all(pos >= tile.origin) && all(pos < tile.origin + i32(TILE_SIZE)) {
            return unpack2x16float(textureLoad(disp, vec2u(pos - tile.origin), i).r);
        }
    }
    return vec2f(0.0);
}

fn load_layer_texel(pos: vec2i, hint: ptr<function, u32>) -> vec4f {
    if *hint < arrayLength(&layer_tile_info) {
        let tile = layer_tile_info[*hint];
        if all(pos >= tile.origin) && all(pos < tile.origin + i32(TILE_SIZE)) {
            return unpack_texel(textureLoad(layer, vec2u(pos - tile.origin), *hint));
        }
    }
    for (var i = 0u; i < arrayLength(&layer_tile_info); i += 1u) {
        let tile = layer_tile_info[i];
        if all(pos >= tile.origin) && all(pos < tile.origin + i32(TILE_SIZE)) {
            *hint = i;
            return unpack_texel(textureLoad(layer, vec2u(pos - tile.origin), i));
        }
    }
    return vec4f(0.0);
}

@if(RGBA8)
fn unpack_texel(texel: vec4u) -> vec4f {
    return unpack_rgba8_texel(texel);
}

@if(A8)
fn unpack_texel(texel: vec4f) -> vec4f {
    return vec4f(texel.r);
}

@if(RGBA8)
fn pack_texel(color: vec4f) -> vec4u {
    return pack_rgba8_texel(color);
}

@if(A8)
fn pack_texel(color: vec4f) -> vec4f {
    return vec4f(color.a, 0.0, 0.0, 0.0);
}

fn sample_layer(pos: vec2f) -> vec4f {
    let p0 = floor(pos);
    let t = pos - p0;
    let i0 = vec2i(p0);
    var hint: u32 = 0xffffffffu;
    let c00 = load_layer_texel(i0, &hint);
    let c10 = load_layer_texel(i0 + vec2i(1, 0), &hint);
    let c01 = load_layer_texel(i0 + vec2i(0, 1), &hint);
    let c11 = load_layer_texel(i0 + vec2i(1, 1), &hint);

    let p00 = vec4f(c00.rgb * c00.a, c00.a);
    let p10 = vec4f(c10.rgb * c10.a, c10.a);
    let p01 = vec4f(c01.rgb * c01.a, c01.a);
    let p11 = vec4f(c11.rgb * c11.a, c11.a);
    let pm = mix(mix(p00, p10, t.x), mix(p01, p11, t.x), t.y);

    if pm.a <= 0.0 {
        return vec4f(0.0);
    }
    return vec4f(pm.rgb / pm.a, pm.a);
}

fn load_selection_texel(pos: vec2i) -> f32 {
    for (var i = 0u; i < arrayLength(&selection_tile_info); i += 1u) {
        let tile = selection_tile_info[i];
        if all(pos >= tile.origin) && all(pos < tile.origin + i32(TILE_SIZE)) {
            return textureLoad(selection, vec2u(pos - tile.origin), i).r;
        }
    }
    return 0.0;
}

fn sample_selection(pos: vec2f) -> f32 {
    let p0 = floor(pos);
    let t = pos - p0;
    let i0 = vec2i(p0);
    let s00 = load_selection_texel(i0);
    let s10 = load_selection_texel(i0 + vec2i(1, 0));
    let s01 = load_selection_texel(i0 + vec2i(0, 1));
    let s11 = load_selection_texel(i0 + vec2i(1, 1));
    return mix(mix(s00, s10, t.x), mix(s01, s11, t.x), t.y);
}

@compute
@workgroup_size(16, 16, 1)
fn render(@builtin(global_invocation_id) global_id: vec3u) {
    let tile_index = global_id.z;
    let local = global_id.xy;
    let pos = output_tile_info[tile_index].origin + vec2i(local);
    let posf = vec2f(pos);

    let f = load_disp_texel(pos);

    var color: vec4f;
    if has_selection != 0u {
        var fragment = sample_layer(posf + f);
        fragment.a *= sample_selection(posf + f);
        var hint: u32 = 0xffffffffu;
        var complement = load_layer_texel(pos, &hint);
        complement.a *= 1.0 - sample_selection(posf);
        color = blend_normal(fragment, complement);
    } else {
        if all(f == vec2f(0.0)) {
            var hint: u32 = 0xffffffffu;
            color = load_layer_texel(pos, &hint);
        } else {
            color = sample_layer(posf + f);
        }
    }

    textureStore(output, local, tile_index, pack_texel(color));
}
