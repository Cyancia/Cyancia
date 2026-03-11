import image::image_tiling::{TileInfo, TILE_SIZE};
import render::math::{Rect, transform_rect};

struct GraphInput {
    pen_position: vec2f,
}

struct StrokeInfo {
    bound_min: vec2i,
    bound_max: vec2i,
}

@if(!POSTPROCESSING) @group(0) @binding(0) var<storage> graph_input: GraphInput;
// During postprocessing estimation, we can know the bounds of the stroke, but we also needs to estimate the bounds after this postprocess pass.
// But during main pass estimation, we have no idea about the bounds, so this will binding be vacant in main pass estimation.
@if((!POSTPROCESSING && !SIZE_ESTIMATION) || POSTPROCESSING) @group(0) @binding(1) var<storage> stroke_info: StrokeInfo;
@group(0) @binding(2) var textures: binding_array<texture_2d<f32>>;
@group(0) @binding(3) var<storage> target_layer_tile_info: array<TileInfo>;
@group(0) @binding(4) var target_layer: texture_storage_2d_array<r32uint, read>;
@if(!SIZE_ESTIMATION) @group(0) @binding(5) var<storage> outputs_tile_info: array<TileInfo>;
@if(!SIZE_ESTIMATION) @group(0) @binding(6) var outputs: texture_storage_2d_array<r32uint, write>;
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

fn texture_transform_mat(local_index: u32, scale: vec2f, rotate: f32, translate: vec2f, anchor: vec2f) -> mat3x3f {
    let s = sin(rotate);
    let c = cos(rotate);
    // let mat = mat3x3f(
    //     c / scale.x, -s / scale.y, 0.0,
    //     s / scale.x, c / scale.y, 0.0,
    //     -(c * translate.x + s * translate.y) / scale.x, (s * translate.x + c * translate.y) / scale.y, 1.0,
    // );
    let anchor_offset = anchor * vec2f(textureDimensions(textures[local_index]));
    let mat = mat3x3f(
        c / scale.x, -s / scale.y, 0.0,
        s / scale.x, c / scale.y, 0.0,
        translate.x - anchor_offset.x, translate.y - anchor_offset.y, 1.0,
    );
    return mat;
}

fn sample_transformed_local_texture(local_index: u32, pixel: vec2f, scale: vec2f, rotate: f32, translate: vec2f, anchor: vec2f) -> vec4f {
    let mat = texture_transform_mat(local_index, scale, rotate, translate, anchor);
    let tex_px = (render::math::inverse_mat3x3(mat) * vec3f(pixel, 1.0)).xy;
    return textureLoad(textures[local_index], vec2i(tex_px), 0);
}

fn sample_local_texture(local_index: u32, pixel: vec2u) -> vec4f {
    return textureLoad(textures[local_index], pixel, 0);
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

fn set_output_color_within_mask(pixel: vec2i, color: vec4f, mask_local_index: u32, mask_scale: vec2f, mask_rotation: f32, mask_translation: vec2f, mask_anchor: vec2f) {
    let tex_size = vec2f(textureDimensions(textures[mask_local_index]));
    let mat = texture_transform_mat(mask_local_index, mask_scale, mask_rotation, mask_translation, mask_anchor);
        
    @if(SIZE_ESTIMATION) {
        let rect = transform_rect(Rect(vec2f(0.0), tex_size), mat);
        require_bounds(vec2i(rect.min), vec2i(rect.max));
    } @else {
        let tex_px = (render::math::inverse_mat3x3(mat) * vec3f(vec2f(pixel), 1.0)).xy;
        if (tex_px.x < 0.0 || tex_px.x >= tex_size.x || tex_px.y < 0.0 || tex_px.y >= tex_size.y) {
            return;
        }
        let mask_value = textureLoad(textures[mask_local_index], vec2i(tex_px), 0).r;
        set_output_color(pixel, vec4f(color * mask_value));
    }
}

fn set_output_color_within_bounds(pixel: vec2i, color: vec4f, bounds_min: vec2f, bounds_max: vec2f) {
    @if(SIZE_ESTIMATION) {
        require_bounds(vec2i(bounds_min), vec2i(bounds_max));
    } @else {
        let pixelf = vec2f(pixel);
        if (pixelf.x < bounds_min.x || pixelf.x >= bounds_max.x || pixelf.y < bounds_min.y || pixelf.y >= bounds_max.y) {
            return;
        }
        set_output_color(pixel, color);
    }
}

@if(!SIZE_ESTIMATION)
fn set_output_color(pixel_pos: vec2i, color: vec4f) {
    let tile_index = convert_pixel_to_output_tile(pixel_pos);
    if tile_index.z == 0xFFFFFFFFu {
        return;
    }
    textureStore(outputs, vec2i(tile_index.xy), tile_index.z, image::texture_unpack::pack_rgba8_texel(color));
}

@if(!SIZE_ESTIMATION)
@compute
@workgroup_size(16, 16, 1)
fn main(@builtin(global_invocation_id) id: vec3u) {
    if any(id.xy >= vec2u(stroke_info.bound_max - stroke_info.bound_min)) {
        return;
    }
    let pixel_pos = vec2i(id.xy) + stroke_info.bound_min;
    let pixel_posf = vec2f(pixel_pos);

    //CODEGENFLAG_COMPILED_GRAPH
}

@if(SIZE_ESTIMATION) @group(0) @binding(16) var<storage, read_write> estimated_affected: vec4i;

@if(SIZE_ESTIMATION)
fn require_bounds(pixel_min: vec2i, pixel_max: vec2i) {
    estimated_affected = vec4i(
        min(estimated_affected.xy, pixel_min),
        max(estimated_affected.zw, pixel_max + 1)
    );
}

@if(SIZE_ESTIMATION)
@compute
@workgroup_size(1, 1, 1)
fn estimate(@builtin(global_invocation_id) id: vec3u) {
    let pixel_pos = vec2i(0);
    let pixel_posf = vec2f(pixel_pos);
    estimated_affected = vec4i(0x7FFFFFFF, 0x7FFFFFFF, -0x7FFFFFFF, -0x7FFFFFFF);

    //CODEGENFLAG_COMPILED_GRAPH
}
