import image::image_tiling::{TileInfo, TILE_SIZE};
import render::math::{Rect, IRect, URect, transform_rect, unsigned_div_ceil, sdf_ellipse, scale_rotate_transform_anchored, scale_rotate_transform_anchored_inv, rotate_mat2x2};
import package::brush_types::{StrokeInfo, StrokePostprocessData, OutputSamples, DabInfo, ComputedPenInput, PassFence, TILE_ALLOCATION_WORKGROUP_SIZE, MAIN_WORKGROUP_SIZE, PostprocessInfo, CanvasResources};

@if(!BOUNDS_EVAL || POSTPROCESS) @group(0) @binding(0) var<storage> sample: GraphInput;
@if(BOUNDS_EVAL && !POSTPROCESS) @group(0) @binding(0) var<storage> sample: OutputSamples;
@group(0) @binding(1) var textures: texture_2d<f32>;
@group(0) @binding(2) var<storage> texture_bounds: array<URect>;
@group(0) @binding(3) var<storage> target_layer_tile_info: array<TileInfo>;
@group(0) @binding(4) var target_layer: texture_storage_2d_array<r32uint, read>;
@if(!BOUNDS_EVAL) @group(0) @binding(5) var<storage> buffer_tile_info: array<TileInfo>;
@if(!BOUNDS_EVAL) @group(0) @binding(6) var stroke_buffer_src: texture_storage_2d_array<r32uint, read>;
@if(!BOUNDS_EVAL) @group(0) @binding(7) var stroke_buffer_dst: texture_storage_2d_array<r32uint, write>;
@if(!BOUNDS_EVAL || POSTPROCESS) @group(0) @binding(8) var<storage, read_write> dab_info: DabInfo;
@if(BOUNDS_EVAL && !POSTPROCESS) @group(0) @binding(8) var<storage, read_write> dab_info: array<DabInfo>;
@group(0) @binding(9) var selection: texture_storage_2d_array<r8unorm, read>;
@group(0) @binding(10) var<storage> selection_tile_info: array<TileInfo>;
@group(0) @binding(11) var<storage> has_selection: u32;
@group(0) @binding(12) var<storage> canvas_resources: CanvasResources;
@group(0) @binding(13) var<storage> initial_pen_input: ComputedPenInput;

//CODEGENFLAG_EXTERNAL_VARIABLE_BINDINGS

struct Mask {
    bounds: Rect,
    value: f32,
}

@if(POSTPROCESS) alias GraphInput = StrokePostprocessData;
@if(!POSTPROCESS) alias GraphInput = ComputedPenInput;

fn main_graph(graph_input: GraphInput, pixel_pos: vec2i, pixel_posf: vec2f) {
    //CODEGENFLAG_COMPILED_GRAPH
}

fn atlas_size(local_index: u32) -> vec2u {
    let bounds = texture_bounds[local_index];
    return bounds.max - bounds.min;
}

fn elliptical_mask(pixel: vec2f, center: vec2f, ab: vec2f, rotation: f32) -> Mask {
    let p0 = pixel - center;
    let p1 = rotate_mat2x2(rotation) * p0;
    let mx = vec2f(max(ab.x, ab.y));

    let dist = sdf_ellipse(p1, vec2f(0.0), ab);
    let value = smoothstep(1.0, 0.0, dist);
    return Mask(Rect(center - mx, center + mx), value);
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

@if(!BOUNDS_EVAL)
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
    @if(BOUNDS_EVAL) {
        return vec4f(0.0);
    }

    @if(!BOUNDS_EVAL) {
        let tile_index = convert_pixel_to_buffer_tile(pixel_pos);
        if tile_index.z == 0xFFFFFFFFu {
            return vec4f(0.0);
        }

        let color = textureLoad(stroke_buffer_src, vec2i(tile_index.xy), tile_index.z);
        return image::texture_unpack::unpack_rgba8_texel(color);
    }
}

fn target_layer_color(pixel_pos: vec2i) -> vec4f {
    let tile_index = convert_pixel_to_layer_tile(pixel_pos);
    if tile_index.z == 0xFFFFFFFFu {
        return vec4f(0.0);
    }
    return image::texture_unpack::unpack_rgba8_texel(textureLoad(target_layer, vec2i(tile_index.xy), tile_index.z));
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

fn load_selection_mask_value(pixel_pos: vec2i) -> f32 {
    if has_selection == 0u {
        return 1.0;
    }

    for (var i = 0u; i < arrayLength(&selection_tile_info); i += 1u) {
        let tile = selection_tile_info[i];
        if any(pixel_pos < tile.origin) || any(pixel_pos >= tile.origin + i32(TILE_SIZE)) {
            continue;
        }

        return textureLoad(selection, pixel_pos - tile.origin, i).r;
    }

    return 0.0;
}

fn set_output_color(pixel_pos: vec2i, color: vec4f) {
    @if(!BOUNDS_EVAL) {
        let tile_index = convert_pixel_to_buffer_tile(pixel_pos);
        if tile_index.z == 0xFFFFFFFFu {
            return;
        }
        let packed = image::texture_unpack::pack_rgba8_texel(color);
        textureStore(stroke_buffer_dst, vec2i(tile_index.xy), tile_index.z, packed);
    }
}

@if(BOUNDS_EVAL) var<private> dab_index: u32 = 0u;

fn set_output_pixel_bounds(bounds: Rect) {
    @if(BOUNDS_EVAL) {
        let tile_min = vec2i(bounds.min) / i32(TILE_SIZE);
        let tile_max = (vec2i(bounds.max) - 1) / i32(TILE_SIZE) + 1;

        @if(POSTPROCESS) {
            dab_info = DabInfo(tile_min, tile_max);
        }
        @if(!POSTPROCESS) {
            dab_info[dab_index] = DabInfo(tile_min, tile_max);
        }
    }
}

@if(!BOUNDS_EVAL)
@compute
@workgroup_size(16, 16, 1)
fn main(@builtin(global_invocation_id) id: vec3u) {
    let layer = id.z;
    let pixel_pos = vec2i(id.xy) + buffer_tile_info[layer].origin;
    if any(pixel_pos < dab_info.bound_min * i32(TILE_SIZE)) || any(pixel_pos >= dab_info.bound_max * i32(TILE_SIZE)) {
        set_output_color(pixel_pos, current_input_color(pixel_pos));
        return;
    }
    let pixel_posf = vec2f(pixel_pos);

    main_graph(sample, pixel_pos, pixel_posf);
}

@if(BOUNDS_EVAL)
@compute
@workgroup_size(1, 1, 16)
fn main_bounds_eval(@builtin(global_invocation_id) id: vec3u) {
    dab_index = id.z;
    @if(POSTPROCESS) {
        if dab_index == 0u {
            main_graph(sample, vec2i(0), vec2f(0.0));
        }
    }
    @if(!POSTPROCESS) {
        if dab_index < sample.n_samples {
            main_graph(sample.samples[dab_index], vec2i(0), vec2f(0.0));
        }
    }
}
