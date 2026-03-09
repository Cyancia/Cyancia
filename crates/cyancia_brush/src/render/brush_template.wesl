import image::image_tiling::{TileInfo, TILE_SIZE};
import render::math::{Rect, transform_rect};

struct GraphInput {
    pen_position: vec2f,
}

struct StrokeInfo {
    shader_origin: vec2i,
    estimated_brush_size: vec2u,
}

@if(!POSTPROCESSING) @group(0) @binding(0) var<storage> graph_input: GraphInput;
@if(!SIZE_ESTIMATION) @group(0) @binding(1) var<storage> stroke_info: StrokeInfo;
@group(0) @binding(2) var textures: binding_array<texture_2d<f32>>;
@if(!SIZE_ESTIMATION) @group(0) @binding(3) var<storage> target_layer_tile_info: array<TileInfo>;
@if(!SIZE_ESTIMATION) @group(0) @binding(4) var target_layer: texture_storage_2d_array<r32uint, read>;
@if(!SIZE_ESTIMATION) @group(0) @binding(5) var<storage> outputs_tile_info: array<TileInfo>;
// TODO: This is weird. This is a bindless array, and theoretically it won't have a fixed size.
//       It doesn't compile if remove the size, but works if it's set to a random value. Why? ｢( • ﾍ •)
@if(!SIZE_ESTIMATION) @group(0) @binding(6) var outputs: texture_storage_2d_array<r32uint, write>;
@if(!SIZE_ESTIMATION) @group(0) @binding(7) var<storage> inputs_tile_info: array<TileInfo>;
@if(!SIZE_ESTIMATION) @group(0) @binding(8) var inputs: texture_storage_2d_array<r32uint, read>;
//CODEGENFLAG_EXTERNAL_VARIABLE_BINDINGS

@if(!SIZE_ESTIMATION)
fn convert_pixel_to_layer_tile(pixel: vec2i) -> vec3u {
    for (var i = 0u; i < arrayLength(&target_layer_tile_info); i = i + 1u) {
        let info = target_layer_tile_info[i];
        if (pixel.x >= info.origin.x && pixel.x < info.origin.x + i32(TILE_SIZE) &&
            pixel.y >= info.origin.y && pixel.y < info.origin.y + i32(TILE_SIZE)) {
            return vec3u(vec2u(pixel - info.origin), i);
        }
    }
    return vec3u(0xffffffffu, 0, 0);
}

@if(!SIZE_ESTIMATION)
fn convert_pixel_to_output_tile(pixel: vec2i) -> vec3u {
    for (var i = 0u; i < arrayLength(&outputs_tile_info); i = i + 1u) {
        let info = outputs_tile_info[i];
        if (pixel.x >= info.origin.x && pixel.x < info.origin.x + i32(TILE_SIZE) &&
            pixel.y >= info.origin.y && pixel.y < info.origin.y + i32(TILE_SIZE)) {;
            return vec3u(vec2u(pixel - info.origin), i);
        }
    }
    return vec3u(0xffffffffu, 0, 0);
}

@if(!SIZE_ESTIMATION)
fn convert_pixel_to_input_tile(pixel: vec2i) -> vec3u {
    for (var i = 0u; i < arrayLength(&inputs_tile_info); i = i + 1u) {
        let info = inputs_tile_info[i];
        if (pixel.x >= info.origin.x && pixel.x < info.origin.x + i32(TILE_SIZE) &&
            pixel.y >= info.origin.y && pixel.y < info.origin.y + i32(TILE_SIZE)) {;
            return vec3u(vec2u(pixel - info.origin), i);
        }
    }
    return vec3u(0xffffffffu, 0, 0);
}

fn paste_texture(layer: u32, cur_pos: vec2f, scale: vec2f, rotate: f32, translate: vec2f, anchor: vec2f) -> vec4f {
    let s = sin(rotate);
    let c = cos(rotate);
    // let mat = mat3x3f(
    //     c / scale.x, -s / scale.y, 0.0,
    //     s / scale.x, c / scale.y, 0.0,
    //     -(c * translate.x + s * translate.y) / scale.x, (s * translate.x + c * translate.y) / scale.y, 1.0,
    // );
    let anchor_offset = anchor * vec2f(textureDimensions(textures[layer]));
    let mat = mat3x3f(
        c / scale.x, -s / scale.y, 0.0,
        s / scale.x, c / scale.y, 0.0,
        translate.x - anchor_offset.x, translate.y - anchor_offset.y, 1.0,
    );
    @if(SIZE_ESTIMATION) {
        let tex_size = vec2f(textureDimensions(textures[layer]));
        let tex_rect = Rect(vec2f(0.0), tex_size);
        let render_rect = transform_rect(tex_rect, mat);
        require_bounds(vec2i(render_rect.min), vec2i(render_rect.max));
        return vec4f(0.0);
    } @else {
        return textureLoad(textures[layer], vec2i((render::math::inverse_mat3x3(mat) * vec3f(cur_pos, 1.0)).xy), 0);
    }
}

fn sample_local_texture(local_index: u32, pixel: vec2u) -> vec4f {
    @if(SIZE_ESTIMATION) {
        return vec4f(0.0);
    } @else {
        return textureLoad(textures[local_index], pixel, 0);
    }
}

fn current_input_color(pixel_pos: vec2i) -> vec4f {
    @if(SIZE_ESTIMATION) {
        return vec4f(0.0);
    } @else {
        let tile_index = convert_pixel_to_input_tile(pixel_pos);
        if tile_index.z == 0xFFFFFFFFu {
            return vec4f(0.0);
        }
        return image::texture_unpack::unpack_rgba8_texel(textureLoad(inputs, vec2i(tile_index.xy), tile_index.z));
    }
}

fn target_layer_color(pixel_pos: vec2i) -> vec4f {
    @if(SIZE_ESTIMATION) {
        return vec4f(0.0);
    } @else {
        let tile_index = convert_pixel_to_layer_tile(pixel_pos);
        if tile_index.z == 0xFFFFFFFFu {
            return vec4f(0.0);
        }
        return image::texture_unpack::unpack_rgba8_texel(textureLoad(target_layer, vec2i(tile_index.xy), tile_index.z));
    }
}

fn set_output_color(pixel_pos: vec2i, color: vec4f) {
    @if(SIZE_ESTIMATION) {
        return;
    } @else {
        let tile_index = convert_pixel_to_output_tile(pixel_pos);
        if tile_index.z == 0xFFFFFFFFu {
            return;
        }
        textureStore(outputs, vec2i(tile_index.xy), tile_index.z, image::texture_unpack::pack_rgba8_texel(color));
    }
}

@if(!SIZE_ESTIMATION)
@compute
@workgroup_size(16, 16, 1)
fn main(@builtin(global_invocation_id) id: vec3u) {
    if (id.x >= stroke_info.estimated_brush_size.x || id.y >= stroke_info.estimated_brush_size.y) {
        return;
    }
    let pixel_pos = vec2i(id.xy) + stroke_info.shader_origin;
    let pixel_posf = vec2f(pixel_pos);

    //CODEGENFLAG_COMPILED_GRAPH
}

const MAX_AFFECTED_TILES: u32 = 256;
@if(SIZE_ESTIMATION) @group(0) @binding(16) var<storage, read_write> estimated_affected_count: u32;
@if(SIZE_ESTIMATION) @group(0) @binding(17) var<storage, read_write> estimated_affected: array<vec2i, MAX_AFFECTED_TILES>;

@if(SIZE_ESTIMATION)
fn require_bounds(pixel_min: vec2i, pixel_max: vec2i) {
    let tile_min = pixel_min / vec2i(TILE_SIZE);
    let tile_max = pixel_max / vec2i(TILE_SIZE) + 1;
    for (var y = tile_min.y; y < tile_max.y; y = y + 1) {
        for (var x = tile_min.x; x < tile_max.x; x = x + 1) {
            estimated_affected[estimated_affected_count] = vec2i(x, y);
            estimated_affected_count = estimated_affected_count + 1u;
        }
    }
}

@if(SIZE_ESTIMATION)
@compute
@workgroup_size(1, 1, 1)
fn estimate(@builtin(global_invocation_id) id: vec3u) {
    let pixel_pos = vec2i(0);
    let pixel_posf = vec2f(pixel_pos);
    estimated_affected_count = 0u;

    //CODEGENFLAG_COMPILED_GRAPH
}
