import package::image::image_tiling::{TileInfo, TILE_SIZE};
import package::render::math::{Rect, IRect, URect, transform_rect, unsigned_div_ceil, sdf_ellipse, scale_rotate_transform_anchored, scale_rotate_transform_anchored_inv};
import package::brush::brush_types::{StrokeInfo, DynamicTileInfo, OutputSamples, DabInfos, ComputedPenInput, PassFence, TILE_ALLOCATION_WORKGROUP_SIZE, MAIN_WORKGROUP_SIZE, PostprocessInfo};

@group(0) @binding(0) var<storage> samples: OutputSamples;
@group(0) @binding(1) var<storage, read_write> stroke_info: StrokeInfo;
@group(0) @binding(2) var textures: texture_2d<f32>;
@group(0) @binding(3) var<storage> texture_bounds: array<URect>;
@group(0) @binding(4) var<storage> target_layer_tile_info: array<TileInfo>;
@group(0) @binding(5) var target_layer: texture_storage_2d_array<r32uint, read>;
@if(!SIZE_ESTIMATION) @group(0) @binding(6) var<storage> buffer_tile_info: DynamicTileInfo;
@if(!SIZE_ESTIMATION) @group(0) @binding(7) var stroke_buffer_src: texture_storage_2d_array<r32uint, read>;
@if(!SIZE_ESTIMATION) @group(0) @binding(8) var stroke_buffer_dst: texture_storage_2d_array<r32uint, write>;
@group(0) @binding(9) var<storage, read_write> dab_infos: DabInfos;

@if(SIZE_ESTIMATION) @group(0) @binding(16) var<storage, read_write> tile_allocation_dispatch: AtomicDispatchParams;
@if(SIZE_ESTIMATION) @group(0) @binding(17) var<storage, read_write> main_dispatch: array<vec3u>;

//CODEGENFLAG_EXTERNAL_VARIABLE_BINDINGS

const N_STROKE_PP: u32 = //CODEGENFLAG_N_STROKE_PP;

struct AtomicDispatchParams {
    x: atomic<u32>,
    y: atomic<u32>,
    z: atomic<u32>,
}

struct CircularMask {
    min_bounds: vec2f,
    max_bounds: vec2f,
    value: f32,
}

fn main_graph(graph_input: ComputedPenInput, pixel_pos: vec2i, pixel_posf: vec2f) {
    //CODEGENFLAG_COMPILED_GRAPH
}

fn atlas_size(local_index: u32) -> vec2u {
    let bounds = texture_bounds[local_index];
    return bounds.max - bounds.min;
}

fn get_accumulated_pixel_bounds() -> IRect {
    @if(SIZE_ESTIMATION) {
        return IRect(
            vec2i(atomicLoad(&stroke_info.accumulated_bound_min_x), atomicLoad(&stroke_info.accumulated_bound_min_y)) * i32(TILE_SIZE),
            vec2i(atomicLoad(&stroke_info.accumulated_bound_max_x), atomicLoad(&stroke_info.accumulated_bound_max_y)) * i32(TILE_SIZE) + 1
        );
    }

    @if(!SIZE_ESTIMATION) {
        return IRect(
            stroke_info.accumulated_bound_min * i32(TILE_SIZE),
            stroke_info.accumulated_bound_max * i32(TILE_SIZE) + 1
        );
    }
}

fn elliptical_mask(pixel: vec2f, center: vec2f, ab: vec2f) -> CircularMask {
    let dist = sdf_ellipse(pixel, center, ab);
    let value = smoothstep(1.0, 0.0, dist);
    return CircularMask(center - ab, center + ab, value);
}

fn convert_pixel_to_layer_tile(pixel: vec2i) -> vec3u {
    for (var i = 0u; i < arrayLength(&target_layer_tile_info); i = i + 1u) {
        let info = target_layer_tile_info[i];
        if pixel.x >= info.origin.x && pixel.x < info.origin.x + i32(TILE_SIZE) &&
            pixel.y >= info.origin.y && pixel.y < info.origin.y + i32(TILE_SIZE) {
            return vec3u(vec2u(pixel - info.origin), i);
        }
    }
    return vec3u(0xffffffffu, 0, 0);
}

fn convert_pixel_to_buffer_tile(pixel: vec2i) -> vec3u {
    for (var i = 0u; i < buffer_tile_info.n_tiles; i = i + 1u) {
        let info = buffer_tile_info.buf[i];
        if pixel.x >= info.origin.x && pixel.x < info.origin.x + i32(TILE_SIZE) &&
            pixel.y >= info.origin.y && pixel.y < info.origin.y + i32(TILE_SIZE) {;
            return vec3u(vec2u(pixel - info.origin), i);
        }
    }
    return vec3u(0xffffffffu, 0, 0);
}

fn texture_transform_mat(local_index: u32, scale: vec2f, rotate: f32, translate: vec2f, anchor: vec2f) -> mat3x3f {
    let tex_size = vec2f(atlas_size(local_index));
    return scale_rotate_transform_anchored(scale, rotate, translate - 0.5 * tex_size, anchor * tex_size);
}

fn texture_transform_mat_inv(local_index: u32, scale: vec2f, rotate: f32, translate: vec2f, anchor: vec2f) -> mat3x3f {
    let tex_size = vec2f(atlas_size(local_index));
    return scale_rotate_transform_anchored_inv(scale, rotate, translate - 0.5 * tex_size, anchor * tex_size);
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
    @if(SIZE_ESTIMATION) {
        return vec4f(0.0);
    }

    @if(!SIZE_ESTIMATION) {
        let tile_index = convert_pixel_to_buffer_tile(pixel_pos);
        if tile_index.z == 0xFFFFFFFFu {
            return vec4f(0.0);
        }

        let color = textureLoad(stroke_buffer_src, vec2i(tile_index.xy), tile_index.z);
        return package::image::texture_unpack::unpack_rgba8_texel(color);
    }
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

    @if(SIZE_ESTIMATION) {
        let rect = transform_rect(Rect(vec2f(0.0), tex_size), texture_transform_mat(mask_local_index, mask_scale, mask_rotation, mask_translation, mask_anchor));
        require_bounds(vec2i(rect.min), vec2i(rect.max));
    }

    let tex_px = (mat * vec3f(vec2f(pixel), 1.0)).xy;
    if tex_px.x < 0.0 || tex_px.x >= tex_size.x || tex_px.y < 0.0 || tex_px.y >= tex_size.y {
        return vec4f(0.0);
    }
    let mask_value = sample_local_texture(mask_local_index, vec2u(tex_px)).r;
    return vec4f(color * mask_value);
}

fn filter_within_bounds(pixel: vec2i, color: vec4f, bounds_min: vec2f, bounds_max: vec2f) -> vec4f {
    @if(SIZE_ESTIMATION) {
        require_bounds(vec2i(bounds_min), vec2i(bounds_max));
    }

    let pixelf = vec2f(pixel);
    if pixelf.x < bounds_min.x || pixelf.x >= bounds_max.x || pixelf.y < bounds_min.y || pixelf.y >= bounds_max.y {
        return vec4f(0.0);
    }
    return color;
}

fn set_output_color(pixel_pos: vec2i, color: vec4f) {
    @if(!SIZE_ESTIMATION) {
        let tile_index = convert_pixel_to_buffer_tile(pixel_pos);
        if tile_index.z == 0xFFFFFFFFu {
            return;
        }
        let packed = package::image::texture_unpack::pack_rgba8_texel(color);
        textureStore(stroke_buffer_dst, vec2i(tile_index.xy), tile_index.z, packed);
    }
}

@if(!SIZE_ESTIMATION)
@compute
@workgroup_size(16, 16, 1)
fn main(@builtin(global_invocation_id) id: vec3u) {
    let dab_index = id.z;

    if dab_index >= dab_infos.n_dabs {
        return;
    }

    let dab_info = dab_infos.buf[dab_index];
    let pixel_pos = vec2i(id.xy) + dab_info.bound_min * i32(TILE_SIZE);
    if any(pixel_pos >= dab_info.bound_max * i32(TILE_SIZE)) {
        return;
    }
    let pixel_posf = vec2f(pixel_pos);
    let graph_input = samples.samples[dab_index];

    main_graph(graph_input, pixel_pos, pixel_posf);
    // set_output_color(pixel_pos, target_layer_color(pixel_pos) + 0.1);
}

@if(SIZE_ESTIMATION) var<private> affected_pixels_precise: IRect;

@if(SIZE_ESTIMATION)
fn require_bounds(pixel_min: vec2i, pixel_max: vec2i) {
    affected_pixels_precise.min = min(affected_pixels_precise.min, pixel_min);
    affected_pixels_precise.max = max(affected_pixels_precise.max, pixel_max);
}

@if(SIZE_ESTIMATION)
@compute
@workgroup_size(1, 1, 8)
fn estimate(@builtin(global_invocation_id) id: vec3u) {
    @if(POSTPROCESS) {
        dab_infos.n_dabs = N_STROKE_PP;
    }

    @if(!POSTPROCESS) {
        dab_infos.n_dabs = samples.n_samples;
    }

    affected_pixels_precise = IRect(vec2i(0x7FFFFFFF), vec2i(-0x7FFFFFFF));

    let dab_index = id.z;
    if dab_index >= dab_infos.n_dabs {
        main_dispatch[dab_index] = vec3u(0);
        return;
    }
    let pixel_pos = vec2i(0);
    let pixel_posf = vec2f(pixel_pos);
    let graph_input = samples.samples[dab_index];
    main_graph(graph_input, pixel_pos, pixel_posf);

    let affected_tiles = IRect(
        affected_pixels_precise.min / i32(TILE_SIZE),
        (affected_pixels_precise.max - 1) / i32(TILE_SIZE) + 1
    );

    dab_infos.buf[dab_index].bound_min = affected_tiles.min;
    dab_infos.buf[dab_index].bound_max = affected_tiles.max;

    atomicMin(&stroke_info.accumulated_bound_min_x, affected_tiles.min.x);
    atomicMin(&stroke_info.accumulated_bound_min_y, affected_tiles.min.y);
    atomicMax(&stroke_info.accumulated_bound_max_x, affected_tiles.max.x);
    atomicMax(&stroke_info.accumulated_bound_max_y, affected_tiles.max.y);

    let affected_tiles_count = vec2u(affected_tiles.max - affected_tiles.min);

    var local_tile_allocation_dispatch = vec3u(
        unsigned_div_ceil(affected_tiles_count.x, TILE_ALLOCATION_WORKGROUP_SIZE.x),
        unsigned_div_ceil(affected_tiles_count.y, TILE_ALLOCATION_WORKGROUP_SIZE.y),
        unsigned_div_ceil(dab_infos.n_dabs, TILE_ALLOCATION_WORKGROUP_SIZE.z)
    );
    atomicMax(&tile_allocation_dispatch.x, local_tile_allocation_dispatch.x);
    atomicMax(&tile_allocation_dispatch.y, local_tile_allocation_dispatch.y);
    atomicMax(&tile_allocation_dispatch.z, local_tile_allocation_dispatch.z);

    let affected_pixels_count = affected_tiles_count * vec2u(TILE_SIZE);
    main_dispatch[dab_index] = vec3u(
        unsigned_div_ceil(affected_pixels_count.x, MAIN_WORKGROUP_SIZE.x),
        unsigned_div_ceil(affected_pixels_count.y, MAIN_WORKGROUP_SIZE.y),
        unsigned_div_ceil(dab_infos.n_dabs, MAIN_WORKGROUP_SIZE.z)
    );
}
