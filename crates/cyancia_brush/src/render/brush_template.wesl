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
@if(!SIZE_ESTIMATION) @group(0) @binding(7) var stroke_buffer_a: texture_storage_2d_array<r32uint, read_write>;
@if(!SIZE_ESTIMATION) @group(0) @binding(8) var stroke_buffer_b: texture_storage_2d_array<r32uint, read_write>;
@group(0) @binding(9) var<storage, read_write> dab_infos: DabInfos;
@if(!SIZE_ESTIMATION) @group(0) @binding(10) var<storage, read_write> fence: PassFence;

@if(SIZE_ESTIMATION) @group(0) @binding(16) var<storage, read_write> tile_allocation_dispatch: vec3u;
@if(SIZE_ESTIMATION) @group(0) @binding(17) var<storage, read_write> main_dispatch: vec3u;

//CODEGENFLAG_EXTERNAL_VARIABLE_BINDINGS

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

fn is_current_input_buffer_a() -> bool {
    @if(POSTPROCESS) {
        return (atomicLoad(&fence.cur_sample) + stroke_info.total_dabs) % 2u == 0;
    }

    @if(!POSTPROCESS) {
        return atomicLoad(&fence.cur_sample) % 2u == 0u;
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

        let color = select(
            textureLoad(stroke_buffer_b, vec2i(tile_index.xy), tile_index.z),
            textureLoad(stroke_buffer_a, vec2i(tile_index.xy), tile_index.z),
            is_current_input_buffer_a()
        );
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
        if is_current_input_buffer_a() {
            textureStore(stroke_buffer_b, vec2i(tile_index.xy), tile_index.z, packed);
        } else {
            textureStore(stroke_buffer_a, vec2i(tile_index.xy), tile_index.z, packed);
        }
    }
}

// TODO: This function is actually not working as expected currently, although it looks correct.
//       As it said in WGPU spec, memory ordering of atomic operations are relaxed, so it's actually impossible to
//       implement something like a global mutex.
//       We're observing some severe artifacts on some dabs, and it seems that this cannot be resolved currently.
//       We may need to wait for the next update of winit and iced, when they are depending on wgpu 28 or later
//       because currently, naga has problem handling atomic operations, and the naga version we are using is patched
//       from a PR.
//       Once everything is updated, we may need to hack around the final generated spirv or somehow by pass the WGPU spec.
//       Not sure how this will be finally solved.
fn wait_for_sample(expected_sample_index: u32) {
    var timeout = 5000u;
    loop {
        let cur_sample_index = atomicLoad(&fence.cur_sample);
        if cur_sample_index == expected_sample_index {
            break;
        }

        if cur_sample_index != expected_sample_index - 1u {
            continue;
        }
        let waiting_for_dab = dab_infos.buf[expected_sample_index - 1u];
        let expected_threads = vec2u(waiting_for_dab.bound_max - waiting_for_dab.bound_min) * TILE_SIZE;
        if atomicCompareExchangeWeak(&fence.cur_sample_finished_threads, expected_threads.x * expected_threads.y, 0u).exchanged {
            atomicStore(&fence.cur_sample, expected_sample_index);
            return;
        }

        timeout -= 1u;
        // TODO: This should not happen... But adding a timeout actually fixes the device loss some times.
        //       So find out why and fix that in the future.
        if timeout == 0u {
            atomicStore(&fence.cur_sample_finished_threads, 0u);
            atomicStore(&fence.cur_sample, expected_sample_index);
            stroke_info.fence_sync_failures += 1u;
            return;
        }
    }
}

fn finish_sample_thread() {
    atomicAdd(&fence.cur_sample_finished_threads, 1u);
}

@if(!SIZE_ESTIMATION && !POSTPROCESS)
@compute
@workgroup_size(16, 16, 1)
fn main(@builtin(global_invocation_id) id: vec3u) {
    let sample_index = id.z;

    let dab_info = dab_infos.buf[sample_index];
    let pixel_pos = vec2i(id.xy) + dab_info.bound_min * i32(TILE_SIZE);
    if any(pixel_pos >= dab_info.bound_max * i32(TILE_SIZE)) {
        return;
    }
    let pixel_posf = vec2f(pixel_pos);
    let graph_input = samples.samples[sample_index];

    wait_for_sample(sample_index);
    main_graph(graph_input, pixel_pos, pixel_posf);
    finish_sample_thread();
}

@if(!SIZE_ESTIMATION && POSTPROCESS)
@compute
@workgroup_size(16, 16, 1)
fn main(@builtin(global_invocation_id) id: vec3u) {
    let bounds = get_accumulated_pixel_bounds();
    let pixel_pos = vec2i(id.xy) + bounds.min;
    if any(pixel_pos >= bounds.max) {
        return;
    }
    let pixel_posf = vec2f(pixel_pos);

    let dummy_input = ComputedPenInput();
    main_graph(dummy_input, pixel_pos, pixel_posf);
}

@if(SIZE_ESTIMATION) var<private> affected_pixels_precise: IRect;

@if(SIZE_ESTIMATION)
fn require_bounds(pixel_min: vec2i, pixel_max: vec2i) {
    affected_pixels_precise.min = min(affected_pixels_precise.min, pixel_min);
    affected_pixels_precise.max = max(affected_pixels_precise.max, pixel_max);
}

@if(SIZE_ESTIMATION && !POSTPROCESS)
@compute
@workgroup_size(1, 1, 8)
fn estimate(@builtin(global_invocation_id) id: vec3u) {
    affected_pixels_precise = IRect(vec2i(0x7FFFFFFF), vec2i(-0x7FFFFFFF));

    let sample_index = id.z;
    if sample_index >= samples.n_samples {
        return;
    }
    let pixel_pos = vec2i(0);
    let pixel_posf = vec2f(pixel_pos);
    let graph_input = samples.samples[sample_index];
    main_graph(graph_input, pixel_pos, pixel_posf);

    let affected_tiles = IRect(
        affected_pixels_precise.min / i32(TILE_SIZE),
        (affected_pixels_precise.max - 1) / i32(TILE_SIZE) + 1
    );

    dab_infos.buf[sample_index].bound_min = affected_tiles.min;
    dab_infos.buf[sample_index].bound_max = affected_tiles.max;

    atomicMin(&stroke_info.accumulated_bound_min_x, affected_tiles.min.x);
    atomicMin(&stroke_info.accumulated_bound_min_y, affected_tiles.min.y);
    atomicMax(&stroke_info.accumulated_bound_max_x, affected_tiles.max.x);
    atomicMax(&stroke_info.accumulated_bound_max_y, affected_tiles.max.y);

    let affected_tiles_count = vec2u(affected_tiles.max - affected_tiles.min);
    atomicMax(&stroke_info.max_affected_tiles_count_x, affected_tiles_count.x);
    atomicMax(&stroke_info.max_affected_tiles_count_y, affected_tiles_count.y);

    storageBarrier();

    // TODO This isn't really correct, add some global barrier.
    if all(id == vec3u(0u)) {
        let max_affected_tiles_count = vec2u(
            atomicLoad(&stroke_info.max_affected_tiles_count_x),
            atomicLoad(&stroke_info.max_affected_tiles_count_y)
        );

        dab_infos.n_dabs = samples.n_samples;

        tile_allocation_dispatch = vec3u(
            unsigned_div_ceil(max_affected_tiles_count.x, TILE_ALLOCATION_WORKGROUP_SIZE.x),
            unsigned_div_ceil(max_affected_tiles_count.y, TILE_ALLOCATION_WORKGROUP_SIZE.y),
            unsigned_div_ceil(samples.n_samples, TILE_ALLOCATION_WORKGROUP_SIZE.z)
        );

        let max_affected_pixels_count = max_affected_tiles_count * vec2u(TILE_SIZE);
        main_dispatch = vec3u(
            unsigned_div_ceil(max_affected_pixels_count.x, MAIN_WORKGROUP_SIZE.x),
            unsigned_div_ceil(max_affected_pixels_count.y, MAIN_WORKGROUP_SIZE.y),
            unsigned_div_ceil(samples.n_samples, MAIN_WORKGROUP_SIZE.z)
        );
    }
}

@if(SIZE_ESTIMATION && POSTPROCESS)
@compute
@workgroup_size(1, 1, 1)
fn estimate() {
    affected_pixels_precise = IRect(vec2i(0x7FFFFFFF), vec2i(-0x7FFFFFFF));

    let pixel_pos = vec2i(0);
    let pixel_posf = vec2f(pixel_pos);
    let dummy_input = ComputedPenInput();
    main_graph(dummy_input, pixel_pos, pixel_posf);

    let affected_tiles = IRect(
        affected_pixels_precise.min / i32(TILE_SIZE),
        (affected_pixels_precise.max - 1) / i32(TILE_SIZE) + 1,
    );

    atomicMin(&stroke_info.accumulated_bound_min_x, affected_tiles.min.x);
    atomicMin(&stroke_info.accumulated_bound_min_y, affected_tiles.min.y);
    atomicMax(&stroke_info.accumulated_bound_max_x, affected_tiles.max.x);
    atomicMax(&stroke_info.accumulated_bound_max_y, affected_tiles.max.y);

    let affected_tiles_count = vec2u(affected_tiles.max - affected_tiles.min);

    tile_allocation_dispatch = vec3u(
        unsigned_div_ceil(affected_tiles_count.x, TILE_ALLOCATION_WORKGROUP_SIZE.x),
        unsigned_div_ceil(affected_tiles_count.y, TILE_ALLOCATION_WORKGROUP_SIZE.y),
        1u
    );

    let affected_pixels_count = affected_tiles_count * vec2u(TILE_SIZE);
    main_dispatch = vec3u(
        unsigned_div_ceil(affected_pixels_count.x, MAIN_WORKGROUP_SIZE.x),
        unsigned_div_ceil(affected_pixels_count.y, MAIN_WORKGROUP_SIZE.y),
        1u
    );
}
