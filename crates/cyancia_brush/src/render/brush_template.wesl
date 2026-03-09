import image::image_tiling::{TileInfo, TILE_SIZE};

struct GraphInput {
    pen_position: vec2f,
}

struct StrokeInfo {
    shader_origin: vec2i,
    estimated_brush_size: vec2u,
}

@if(!POSTPROCESSING) @group(0) @binding(0) var<storage> graph_input: GraphInput;
@group(0) @binding(1) var<storage> stroke_info: StrokeInfo;
@group(0) @binding(2) var textures: binding_array<texture_2d<f32>>;
@group(0) @binding(3) var<storage> target_layer_tile_info: array<TileInfo>;
@group(0) @binding(4) var target_layer: texture_storage_2d_array<r32uint, read>;
@group(0) @binding(5) var<storage> outputs_tile_info: array<TileInfo>;
// TODO: This is weird. This is a bindless array, and theoretically it won't have a fixed size.
//       It doesn't compile if remove the size, but works if it's set to a random value. Why? ｢( • ﾍ •)
@group(0) @binding(6) var outputs: texture_storage_2d_array<r32uint, write>;
@group(0) @binding(7) var<storage> inputs_tile_info: array<TileInfo>;
@group(0) @binding(8) var inputs: texture_storage_2d_array<r32uint, read>;
//CODEGENFLAG_EXTERNAL_VARIABLE_BINDINGS

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

fn inverse_mat3x3(matrix: mat3x3<f32>) -> mat3x3<f32> {
    let tmp0 = cross(matrix[1], matrix[2]);
    let tmp1 = cross(matrix[2], matrix[0]);
    let tmp2 = cross(matrix[0], matrix[1]);
    let inv_det = 1.0 / dot(matrix[2], tmp2);
    return transpose(mat3x3<f32>(tmp0 * inv_det, tmp1 * inv_det, tmp2 * inv_det));
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
    return textureLoad(textures[layer], vec2i((inverse_mat3x3(mat) * vec3f(cur_pos, 1.0)).xy), 0);
}

fn current_input_color(pixel_pos: vec2i) -> vec4f {
    let tile_index = convert_pixel_to_input_tile(pixel_pos);
    if tile_index.z == 0xFFFFFFFFu {
        return vec4f(0.0);
    }
    return image::texture_unpack::unpack_rgba8_texel(textureLoad(inputs, vec2i(tile_index.xy), tile_index.z));
}

fn target_layer_color(pixel_pos: vec2i) -> vec4f {
    let tile_index = convert_pixel_to_layer_tile(pixel_pos);
    if tile_index.z == 0xFFFFFFFFu {
        return vec4f(0.0);
    }
    return image::texture_unpack::unpack_rgba8_texel(textureLoad(target_layer, vec2i(tile_index.xy), tile_index.z));
}

fn set_output_color(pixel_pos: vec2i, color: vec4f) {
    let tile_index = convert_pixel_to_output_tile(pixel_pos);
    if tile_index.z == 0xFFFFFFFFu {
        return;
    }
    textureStore(outputs, vec2i(tile_index.xy), tile_index.z, image::texture_unpack::pack_rgba8_texel(color));
}

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
