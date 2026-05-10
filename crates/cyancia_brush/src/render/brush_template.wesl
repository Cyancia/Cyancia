import package::image::image_tiling::{TileInfo, TILE_SIZE};
import package::render::math::{Rect, IRect, URect, transform_rect, unsigned_div_ceil, sdf_ellipse, scale_rotate_transform_anchored, scale_rotate_transform_anchored_inv};
import package::brush::brush_types::{StrokeInfo, StrokePostprocessData, DynamicTileInfo, OutputSamples, DabInfo, ComputedPenInput, PassFence, TILE_ALLOCATION_WORKGROUP_SIZE, MAIN_WORKGROUP_SIZE, PostprocessInfo};

@if(POSTPROCESS) @group(0) @binding(0) var<storage> sample: StrokePostprocessData;
@if(!POSTPROCESS) @group(0) @binding(0) var<storage> sample: ComputedPenInput;
@group(0) @binding(1) var textures: texture_2d<f32>;
@group(0) @binding(2) var<storage> texture_bounds: array<URect>;
@group(0) @binding(3) var<storage> target_layer_tile_info: array<TileInfo>;
@group(0) @binding(4) var target_layer: texture_storage_2d_array<r32uint, read>;
@group(0) @binding(5) var<storage> buffer_tile_info: array<TileInfo>;
@group(0) @binding(6) var stroke_buffer_src: texture_storage_2d_array<r32uint, read>;
@group(0) @binding(7) var stroke_buffer_dst: texture_storage_2d_array<r32uint, write>;
@group(0) @binding(8) var<storage, read_write> dab_info: DabInfo;

//CODEGENFLAG_EXTERNAL_VARIABLE_BINDINGS

struct AtomicDispatchParams {
    x: atomic<u32>,
    y: atomic<u32>,
    z: atomic<u32>,
}

struct Mask {
    bounds: Rect,
    value: f32,
}

fn main_graph(graph_input: ComputedPenInput, pixel_pos: vec2i, pixel_posf: vec2f) {
    //CODEGENFLAG_COMPILED_GRAPH
}

fn atlas_size(local_index: u32) -> vec2u {
    let bounds = texture_bounds[local_index];
    return bounds.max - bounds.min;
}

fn elliptical_mask(pixel: vec2f, center: vec2f, ab: vec2f) -> Mask {
    let dist = sdf_ellipse(pixel, center, ab);
    let value = smoothstep(1.0, 0.0, dist);
    return Mask(Rect(center - ab, center + ab), value);
}

fn convert_pixel_to_layer_tile(pixel: vec2i) -> vec3u {
    for (var i = 0u; i < arrayLength(&target_layer_tile_info); i = i + 1u) {
        let info = target_layer_tile_info[i];
        if pixel.x >= info.origin.x && pixel.x < info.origin.x + i32(TILE_SIZE) &&
            pixel.y >= info.origin.y && pixel.y < info.origin.y + i32(TILE_SIZE) {
            return vec3u(vec2u(pixel - info.origin), i);
        }
    }
    return vec3u(0, 0, 0xFFFFFFFFu);
}

fn convert_pixel_to_buffer_tile(pixel: vec2i) -> vec3u {
    for (var i = 0u; i < arrayLength(&buffer_tile_info); i = i + 1u) {
        let info = buffer_tile_info[i];
        if pixel.x >= info.origin.x && pixel.x < info.origin.x + i32(TILE_SIZE) &&
            pixel.y >= info.origin.y && pixel.y < info.origin.y + i32(TILE_SIZE) {;
            return vec3u(vec2u(pixel - info.origin), i);
        }
    }
    return vec3u(0, 0, 0xFFFFFFFFu);
}

fn texture_transform_mat(local_index: u32, scale: vec2f, rotate: f32, translate: vec2f, anchor: vec2f) -> mat3x3f {
    let tex_size = vec2f(atlas_size(local_index));
    return scale_rotate_transform_anchored(scale, rotate, translate, anchor * tex_size);
}

fn texture_transform_mat_inv(local_index: u32, scale: vec2f, rotate: f32, translate: vec2f, anchor: vec2f) -> mat3x3f {
    let tex_size = vec2f(atlas_size(local_index));
    return scale_rotate_transform_anchored_inv(scale, rotate, translate, anchor * tex_size);
}

fn sample_transformed_local_texture_clamp(local_index: u32, pixel: vec2f, scale: vec2f, rotate: f32, translate: vec2f, anchor: vec2f) -> vec4f {
    let mat = texture_transform_mat_inv(local_index, scale, rotate, translate, anchor);
    let tex_px = (mat * vec3f(pixel, 1.0)).xy;
    return sample_local_texture(local_index, vec2u(tex_px));
}

fn sample_transformed_local_texture_wrap(local_index: u32, pixel: vec2f, scale: vec2f, rotate: f32, translate: vec2f, anchor: vec2f) -> vec4f {
    let mat = texture_transform_mat_inv(local_index, scale, rotate, translate, anchor);
    let tex_px = (mat * vec3f(pixel, 1.0)).xy;
    let tex_size = atlas_size(local_index);
    let wrapped_tex_px = abs(vec2i(tex_px) % vec2i(tex_size));
    return sample_local_texture(local_index, vec2u(wrapped_tex_px));
}

fn sample_local_texture(local_index: u32, pixel: vec2u) -> vec4f {
    let bounds = texture_bounds[local_index];
    if any(pixel >= bounds.max - bounds.min) {
        return vec4f(0.0);
    }
    return textureLoad(textures, pixel + bounds.min, 0);
}

fn current_input_color(pixel_pos: vec2i) -> vec4f {
    let tile_index = convert_pixel_to_buffer_tile(pixel_pos);
    if tile_index.z == 0xFFFFFFFFu {
        return vec4f(0.0);
    }

    let color = textureLoad(stroke_buffer_src, vec2i(tile_index.xy), tile_index.z);
    return package::image::texture_unpack::unpack_rgba8_texel(color);
}

fn target_layer_color(pixel_pos: vec2i) -> vec4f {
    let tile_index = convert_pixel_to_layer_tile(pixel_pos);
    if tile_index.z == 0xFFFFFFFFu {
        return vec4f(0.0);
    }
    return package::image::texture_unpack::unpack_rgba8_texel(textureLoad(target_layer, vec2i(tile_index.xy), tile_index.z));
}

fn filter_within_mask(pixel: vec2i, color: vec4f, mask_local_index: u32, mask_scale: vec2f, mask_rotation: f32, mask_translation: vec2f, mask_anchor: vec2f) -> vec4f {
    let tex_size = vec2f(atlas_size(mask_local_index));
    let mat = texture_transform_mat_inv(mask_local_index, mask_scale, mask_rotation, mask_translation, mask_anchor);

    let tex_px = (mat * vec3f(vec2f(pixel), 1.0)).xy;
    if tex_px.x < 0.0 || tex_px.x >= tex_size.x || tex_px.y < 0.0 || tex_px.y >= tex_size.y {
        return vec4f(0.0);
    }
    let mask_value = sample_local_texture(mask_local_index, vec2u(tex_px)).r;
    return vec4f(color * mask_value);
}

fn filter_within_mask_bounds(mask_local_index: u32, mask_scale: vec2f, mask_rotation: f32, mask_translation: vec2f, mask_anchor: vec2f) -> Rect {
    let tex_size = vec2f(atlas_size(mask_local_index));
    let rect = transform_rect(Rect(vec2f(0.0), tex_size), texture_transform_mat(mask_local_index, mask_scale, mask_rotation, mask_translation, mask_anchor));
    return rect;
}

fn filter_within_bounds(pixel: vec2i, color: vec4f, bounds: Rect) -> vec4f {
    let pixelf = vec2f(pixel);
    if any(pixelf < bounds.min) || any(pixelf >= bounds.max) {
        return vec4f(0.0);
    }
    return color;
}

fn set_output_color(pixel_pos: vec2i, color: vec4f) {
    let tile_index = convert_pixel_to_buffer_tile(pixel_pos);
    if tile_index.z == 0xFFFFFFFFu {
        return;
    }
    let packed = package::image::texture_unpack::pack_rgba8_texel(color);
    textureStore(stroke_buffer_dst, vec2i(tile_index.xy), tile_index.z, packed);
}

@compute
@workgroup_size(16, 16, 1)
fn main(@builtin(global_invocation_id) id: vec3u) {
    let pixel_pos = vec2i(id.xy) + dab_info.bound_min;
    if any(pixel_pos >= dab_info.bound_max) {
        return;
    }
    let pixel_posf = vec2f(pixel_pos);

    @if(POSTPROCESS) {
        main_graph(ComputedPenInput(), pixel_pos, pixel_posf);
    }

    @if(!POSTPROCESS) {
        main_graph(sample, pixel_pos, pixel_posf);
        // set_output_color(pixel_pos, current_input_color(pixel_pos) + vec4f(0.1));
    }
}
