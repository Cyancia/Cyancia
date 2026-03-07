struct GraphInput {
    pen_position: vec2f,
}

struct StrokeInfo {
    shader_origin: vec2i,
    estimated_brush_size: vec2u,
    tile_size: u32,
}

struct TileInfo {
    tile_origin: vec2i,
}

@if(!POSTPROCESSING) @group(0) @binding(0) var<storage> graph_input: GraphInput;
@group(0) @binding(1) var<storage> stroke_info: StrokeInfo;
@group(0) @binding(2) var textures: binding_array<texture_2d<f32>>;
@group(0) @binding(3) var<storage> target_layer_tile_info: array<TileInfo>;
@group(0) @binding(4) var target_layer: binding_array<texture_storage_2d<r32uint, read>, 1>;
@group(0) @binding(5) var<storage> tile_info: array<TileInfo>;
// TODO: This is weird. This is a bindless array, and theoretically it won't have a fixed size.
//       It doesn't compile if remove the size, but works if it's set to a random value. Why? ｢( • ﾍ •)
@group(0) @binding(6) var outputs: binding_array<texture_storage_2d<r32uint, write>, 1>;
@if(POSTPROCESSING) @group(0) @binding(7) var inputs: binding_array<texture_storage_2d<r32uint, read>, 1>;
//CODEGENFLAG_EXTERNAL_VARIABLE_BINDINGS

fn convert_pixel_to_layer_tile(pixel: vec2i, layer: ptr<function, u32>, coord: ptr<function, vec2u>) {
    for (var i = 0u; i < arrayLength(&target_layer_tile_info); i = i + 1u) {
        let info = target_layer_tile_info[i];
        if (pixel.x >= info.tile_origin.x && pixel.x < info.tile_origin.x + i32(stroke_info.tile_size) &&
            pixel.y >= info.tile_origin.y && pixel.y < info.tile_origin.y + i32(stroke_info.tile_size)) {
            *layer = i;
            *coord = vec2u(pixel - info.tile_origin);
            return;
        }
    }
}

fn convert_pixel_to_tile(pixel: vec2i, layer: ptr<function, u32>, coord: ptr<function, vec2u>) {
    for (var i = 0u; i < arrayLength(&tile_info); i = i + 1u) {
        let info = tile_info[i];
        if (pixel.x >= info.tile_origin.x && pixel.x < info.tile_origin.x + i32(stroke_info.tile_size) &&
            pixel.y >= info.tile_origin.y && pixel.y < info.tile_origin.y + i32(stroke_info.tile_size)) {
            *layer = i;
            *coord = vec2u(pixel - info.tile_origin);
            return;
        }
    }
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
    var cur_layer = 0u;
    var cur_coord = vec2u(0u);
    convert_pixel_to_tile(pixel_pos, &cur_layer, &cur_coord);
    return image::texture_unpack::unpack_rgba8_texel(textureLoad(inputs[cur_layer], vec2i(cur_coord)));
}

fn target_layer_color(pixel_pos: vec2i) -> vec4f {
    var cur_layer = 0u;
    var cur_coord = vec2u(0u);
    convert_pixel_to_layer_tile(pixel_pos, &cur_layer, &cur_coord);
    return image::texture_unpack::unpack_rgba8_texel(textureLoad(target_layer[0], vec2i(cur_coord)));
}

fn set_output_color(pixel_pos: vec2i, color: vec4f) {
    var cur_layer = 0u;
    var cur_coord = vec2u(0u);
    convert_pixel_to_tile(pixel_pos, &cur_layer, &cur_coord);
    textureStore(outputs[cur_layer], vec2i(cur_coord), image::texture_unpack::pack_rgba8_texel(color));
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
