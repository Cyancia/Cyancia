Compiled brush preset:
-------------- Compiled brush preset --------------
-------------- Input sampling shader --------------
@group(0) @binding(0)
var<storage> new_input: package_brush__1brush_types_PenInput;

@group(0) @binding(1)
var<storage, read_write> input_sampler: package_brush__1brush_types_PenInputSampler;

@group(0) @binding(2)
var<storage, read_write> output_samples: package_brush__1brush_types_OutputSamples;

@group(0) @binding(3)
var<storage, read_write> estimate_dispatch: vec3u;

@group(0) @binding(4)
var<storage, read_write> stroke_info: package_brush__1brush_types_StrokeInfo;

fn compute_spacing_factor(src: package_brush__1brush_types_ComputedPenInput, dst: package_brush__1brush_types_ComputedPenInput) -> f32 {
    return distance(src.position, dst.position);
}

fn compute_required_spacing(sample: package_brush__1brush_types_ComputedPenInput) -> f32 {
    return 1.0;
}

@compute @workgroup_size(1, 1, 1)
fn main() {
    output_samples.n_samples = 0;
    let new_sample = package_brush__1brush_types_ComputedPenInput(new_input.position);
    if input_sampler.has_last_sample == 0 {
        input_sampler.last_input = new_input;
        input_sampler.last_sample = new_sample;
        input_sampler.has_last_sample = 1;
        output_samples.samples[output_samples.n_samples] = new_sample;
        output_samples.n_samples += 1;
        return;
    }
    let spacing = compute_required_spacing(new_sample);
    let total_spacing = compute_spacing_factor(new_sample, input_sampler.last_sample);
    var remaining_spacing = total_spacing;
    while (output_samples.n_samples < arrayLength(&output_samples.samples)) {
        let t = (total_spacing - remaining_spacing) / total_spacing;
        let interpolated_sample = package_brush__1brush_types_ComputedPenInput(mix(input_sampler.last_sample.position, new_sample.position, t));
        output_samples.samples[output_samples.n_samples] = interpolated_sample;
        output_samples.n_samples += 1;
        remaining_spacing -= spacing;
        if remaining_spacing < spacing {
            input_sampler.last_input = new_input;
            input_sampler.last_sample = interpolated_sample;
            break;
        }
    }
    estimate_dispatch = vec3u(1, 1, package_render_math__2unsigned_div_ceil(output_samples.n_samples, package_brush__1brush_types__2ESTIMATION_WORKGROUP_SIZE.z));
    stroke_info.total_dabs += output_samples.n_samples;
}

const package_brush__1brush_types__2ESTIMATION_WORKGROUP_SIZE = vec3u(1, 1, 8);

struct package_brush__1brush_types_StrokeInfo {
    accumulated_bound_min: vec2i,
    accumulated_bound_max: vec2i,
    max_affected_tiles_count: vec2u,
    total_dabs: u32,
    _padding: u32
}

struct package_brush__1brush_types_OutputSamples {
    n_samples: u32,
    samples: array<package_brush__1brush_types_ComputedPenInput>
}

struct package_brush__1brush_types_ComputedPenInput {
    position: vec2f
}

struct package_brush__1brush_types_PenInputSampler {
    last_input: package_brush__1brush_types_PenInput,
    last_sample: package_brush__1brush_types_ComputedPenInput,
    has_last_sample: u32
}

struct package_brush__1brush_types_PenInput {
    position: vec2f
}

fn package_render_math__2unsigned_div_ceil(lhs: u32, rhs: u32) -> u32 {
    return (lhs + rhs - 1u) / rhs;
}

-------------- Main graph shader --------------
-------------- Shader --------------
@group(0) @binding(0)
var<storage> samples: package_brush__1brush_types_OutputSamples;

@group(0) @binding(2)
var textures: binding_array<texture_2d<f32>>;

@group(0) @binding(5)
var<storage> buffer_tile_info: package_brush__1brush_types_DynamicTileInfo;

@group(0) @binding(6)
var stroke_buffer_a: texture_storage_2d_array<r32uint, read_write>;

@group(0) @binding(7)
var stroke_buffer_b: texture_storage_2d_array<r32uint, read_write>;

@group(0) @binding(8)
var<storage, read_write> dab_infos: package_brush__1brush_types_DabInfos;

@group(0) @binding(9)
var<storage, read_write> fence: package_brush__1brush_types_PassFence;

fn main_graph(graph_input: package_brush__1brush_types_ComputedPenInput, pixel_pos: vec2i, pixel_posf: vec2f) {
    let output_0_ = graph_input.position;
    let output_1_ = 1u;
    let output_2_ = filter_within_mask(pixel_pos, vec4f(1.0, 0.0, 0.0, 1.0), output_1_, vec2f(1.0, 1.0), 0.0, output_0_, vec2f(0.5, 0.5));
    let output_3_ = current_input_color(pixel_pos);
    let output_4_ = package_image__1blend_modes__1blend_normal(output_2_, vec4f(output_3_.rgb, output_3_.a * 0.49));
    set_output_color(pixel_pos, output_4_);
}

fn is_current_input_buffer_a() -> bool {
    {
        return atomicLoad(&fence.cur_sample) % 2u == 0u;
    }
}

fn convert_pixel_to_buffer_tile(pixel: vec2i) -> vec3u {
    for (var i = 0u; i < buffer_tile_info.n_tiles; i = i + 1u) {
        let info = buffer_tile_info.buf[i];
        if pixel.x >= info.origin.x && pixel.x < info.origin.x + i32(package_image__1image_tiling__1TILE_SIZE) && pixel.y >= info.origin.y && pixel.y < info.origin.y + i32(package_image__1image_tiling__1TILE_SIZE) {
            return vec3u(vec2u(pixel - info.origin), i);
        }
    }
    return vec3u(4294967295u, 0, 0);
}

fn texture_transform_mat(local_index: u32, scale: vec2f, rotate: f32, translate: vec2f, anchor: vec2f) -> mat3x3f {
    let s = sin(rotate);
    let c = cos(rotate);
    let anchor_offset = anchor * vec2f(textureDimensions(textures[local_index]));
    let mat = mat3x3f(c / scale.x, -s / scale.y, 0.0, s / scale.x, c / scale.y, 0.0, translate.x - anchor_offset.x, translate.y - anchor_offset.y, 1.0);
    return mat;
}

fn current_input_color(pixel_pos: vec2i) -> vec4f {
    {
        let tile_index = convert_pixel_to_buffer_tile(pixel_pos);
        if tile_index.z == 4294967295u {
            return vec4f(0.0);
        }
        let color = select(textureLoad(stroke_buffer_b, vec2i(tile_index.xy), tile_index.z), textureLoad(stroke_buffer_a, vec2i(tile_index.xy), tile_index.z), is_current_input_buffer_a());
        return package_image__1texture_unpack__2unpack_rgba8_texel(color);
    }
}

fn filter_within_mask(pixel: vec2i, color: vec4f, mask_local_index: u32, mask_scale: vec2f, mask_rotation: f32, mask_translation: vec2f, mask_anchor: vec2f) -> vec4f {
    let tex_size = vec2f(textureDimensions(textures[mask_local_index]));
    let mat = texture_transform_mat(mask_local_index, mask_scale, mask_rotation, mask_translation, mask_anchor);
    let tex_px = (package_render_math__1inverse_mat3x3(mat) * vec3f(vec2f(pixel), 1.0)).xy;
    if tex_px.x < 0.0 || tex_px.x >= tex_size.x || tex_px.y < 0.0 || tex_px.y >= tex_size.y {
        return vec4f(0.0);
    }
    let mask_value = textureLoad(textures[mask_local_index], vec2i(tex_px), 0).r;
    return vec4f(color * mask_value);
}

fn set_output_color(pixel_pos: vec2i, color: vec4f) {
    {
        let tile_index = convert_pixel_to_buffer_tile(pixel_pos);
        if tile_index.z == 4294967295u {
            return;
        }
        let packed = package_image__1texture_unpack__2pack_rgba8_texel(color);
        if is_current_input_buffer_a() {
            textureStore(stroke_buffer_b, vec2i(tile_index.xy), tile_index.z, packed);
        }
        else {
            textureStore(stroke_buffer_a, vec2i(tile_index.xy), tile_index.z, packed);
        }
    }
}

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
        let expected_threads = vec2u(waiting_for_dab.bound_max - waiting_for_dab.bound_min) * package_image__1image_tiling__1TILE_SIZE;
        if atomicCompareExchangeWeak(&fence.cur_sample_finished_threads, expected_threads.x * expected_threads.y, 0u).exchanged {
            atomicStore(&fence.cur_sample, expected_sample_index);
            return;
        }
        timeout -= 1u;
        if timeout == 0u {
            atomicStore(&fence.cur_sample_finished_threads, 0u);
            atomicStore(&fence.cur_sample, expected_sample_index);
            return;
        }
    }
}

fn finish_sample_thread() {
    atomicAdd(&fence.cur_sample_finished_threads, 1u);
}

@compute @workgroup_size(16, 16, 1)
fn main(@builtin(global_invocation_id) id: vec3u) {
    let sample_index = id.z;
    let dab_info = dab_infos.buf[sample_index];
    let pixel_pos = vec2i(id.xy) + dab_info.bound_min * i32(package_image__1image_tiling__1TILE_SIZE);
    if any(pixel_pos >= dab_info.bound_max * i32(package_image__1image_tiling__1TILE_SIZE)) {
        return;
    }
    let pixel_posf = vec2f(pixel_pos);
    let graph_input = samples.samples[sample_index];
    wait_for_sample(sample_index);
    main_graph(graph_input, pixel_pos, pixel_posf);
    finish_sample_thread();
}

struct package_brush__1brush_types_DynamicTileInfo {
    n_tiles: u32,
    buf: array<package_image__1image_tiling_TileInfo>
}

struct package_brush__1brush_types_OutputSamples {
    n_samples: u32,
    samples: array<package_brush__1brush_types_ComputedPenInput>
}

struct package_brush__1brush_types_DabInfos {
    n_dabs: u32,
    buf: array<package_brush__1brush_types_DabInfo>
}

struct package_brush__1brush_types_DabInfo {
    bound_min: vec2i,
    bound_max: vec2i
}

struct package_brush__1brush_types_ComputedPenInput {
    position: vec2f
}

struct package_brush__1brush_types_PassFence {
    cur_sample: atomic<u32>,
    cur_sample_finished_threads: atomic<u32>
}

const package_image__1image_tiling__1TILE_SIZE: u32 = 256;

struct package_image__1image_tiling_TileInfo {
    index: vec2i,
    origin: vec2i
}

fn package_render_math__1inverse_mat3x3(matrix: mat3x3<f32>) -> mat3x3<f32> {
    let tmp0 = cross(matrix[1], matrix[2]);
    let tmp1 = cross(matrix[2], matrix[0]);
    let tmp2 = cross(matrix[0], matrix[1]);
    let inv_det = 1.0 / dot(matrix[2], tmp2);
    return transpose(mat3x3<f32>(tmp0 * inv_det, tmp1 * inv_det, tmp2 * inv_det));
}

fn package_image__1texture_unpack__2unpack_rgba8_texel(texel: vec4u) -> vec4f {
    return package_image__1texture_unpack__1unpack_rgba8(texel.r);
}

fn package_image__1texture_unpack__2pack_rgba8_texel(color: vec4f) -> vec4u {
    return vec4u(package_image__1texture_unpack__1pack_rgba8(color), 0, 0, 0);
}

fn package_image__1texture_unpack__1unpack_rgba8(x: u32) -> vec4f {
    let r = f32((x >> 0) & 255) / 255.0;
    let g = f32((x >> 8) & 255) / 255.0;
    let b = f32((x >> 16) & 255) / 255.0;
    let a = f32((x >> 24) & 255) / 255.0;
    return vec4f(r, g, b, a);
}

fn package_image__1texture_unpack__1pack_rgba8(color: vec4f) -> u32 {
    let r = u32(clamp(color.r * 255.0, 0.0, 255.0));
    let g = u32(clamp(color.g * 255.0, 0.0, 255.0));
    let b = u32(clamp(color.b * 255.0, 0.0, 255.0));
    let a = u32(clamp(color.a * 255.0, 0.0, 255.0));
    return (a << 24) | (b << 16) | (g << 8) | r;
}

const package_image__1blend_modes__1BLEND_EPSILON: f32 = 1e-6;

fn package_image__1blend_modes__1clamp01_rgb(color: vec3f) -> vec3f {
    return clamp(color, vec3f(0.0), vec3f(1.0));
}

fn package_image__1blend_modes__1blend_compose(src: vec4f, dst: vec4f, blended_rgb: vec3f) -> vec4f {
    let src_a = clamp(src.a, 0.0, 1.0);
    let dst_a = clamp(dst.a, 0.0, 1.0);
    let out_a = src_a + dst_a * (1.0 - src_a);
    if out_a <= package_image__1blend_modes__1BLEND_EPSILON {
        return vec4f(0.0);
    }
    let out_rgb_premul = src.rgb * src_a * (1.0 - dst_a) + dst.rgb * dst_a * (1.0 - src_a) + blended_rgb * src_a * dst_a;
    let out_rgb = out_rgb_premul / out_a;
    return vec4f(package_image__1blend_modes__1clamp01_rgb(out_rgb), out_a);
}

fn package_image__1blend_modes__1blend_normal(src: vec4f, dst: vec4f) -> vec4f {
    return package_image__1blend_modes__1blend_compose(src, dst, src.rgb);
}

-------------- Size estimation --------------
@group(0) @binding(0)
var<storage> samples: package_brush__1brush_types_OutputSamples;

@group(0) @binding(1)
var<storage, read_write> stroke_info: package_brush__1brush_types_StrokeInfo;

@group(0) @binding(2)
var textures: binding_array<texture_2d<f32>>;

@group(0) @binding(8)
var<storage, read_write> dab_infos: package_brush__1brush_types_DabInfos;

@group(0) @binding(16)
var<storage, read_write> tile_allocation_dispatch: vec3u;

@group(0) @binding(17)
var<storage, read_write> main_dispatch: vec3u;

fn main_graph(graph_input: package_brush__1brush_types_ComputedPenInput, pixel_pos: vec2i, pixel_posf: vec2f) {
    let output_0_ = graph_input.position;
    let output_1_ = 1u;
    let output_2_ = filter_within_mask(pixel_pos, vec4f(1.0, 0.0, 0.0, 1.0), output_1_, vec2f(1.0, 1.0), 0.0, output_0_, vec2f(0.5, 0.5));
    let output_3_ = current_input_color(pixel_pos);
    let output_4_ = package_image__1blend_modes__1blend_normal(output_2_, vec4f(output_3_.rgb, output_3_.a * 0.49));
    set_output_color(pixel_pos, output_4_);
}

fn texture_transform_mat(local_index: u32, scale: vec2f, rotate: f32, translate: vec2f, anchor: vec2f) -> mat3x3f {
    let s = sin(rotate);
    let c = cos(rotate);
    let anchor_offset = anchor * vec2f(textureDimensions(textures[local_index]));
    let mat = mat3x3f(c / scale.x, -s / scale.y, 0.0, s / scale.x, c / scale.y, 0.0, translate.x - anchor_offset.x, translate.y - anchor_offset.y, 1.0);
    return mat;
}

fn current_input_color(pixel_pos: vec2i) -> vec4f {
    {
        return vec4f(0.0);
    }
}

fn filter_within_mask(pixel: vec2i, color: vec4f, mask_local_index: u32, mask_scale: vec2f, mask_rotation: f32, mask_translation: vec2f, mask_anchor: vec2f) -> vec4f {
    let tex_size = vec2f(textureDimensions(textures[mask_local_index]));
    let mat = texture_transform_mat(mask_local_index, mask_scale, mask_rotation, mask_translation, mask_anchor);
    {
        let rect = package_render_math__1transform_rect(package_render_math_Rect(vec2f(0.0), tex_size), mat);
        require_bounds(vec2i(rect.min), vec2i(rect.max));
    }
    let tex_px = (package_render_math__1inverse_mat3x3(mat) * vec3f(vec2f(pixel), 1.0)).xy;
    if tex_px.x < 0.0 || tex_px.x >= tex_size.x || tex_px.y < 0.0 || tex_px.y >= tex_size.y {
        return vec4f(0.0);
    }
    let mask_value = textureLoad(textures[mask_local_index], vec2i(tex_px), 0).r;
    return vec4f(color * mask_value);
}

fn set_output_color(pixel_pos: vec2i, color: vec4f) {

}

var<private> affected_pixels_precise: package_render_math_IRect;

fn require_bounds(pixel_min: vec2i, pixel_max: vec2i) {
    affected_pixels_precise.min = min(affected_pixels_precise.min, pixel_min);
    affected_pixels_precise.max = max(affected_pixels_precise.max, pixel_max);
}

@compute @workgroup_size(1, 1, 8)
fn estimate(@builtin(global_invocation_id) id: vec3u) {
    affected_pixels_precise = package_render_math_IRect(vec2i(2147483647), vec2i(-2147483647));
    let sample_index = id.z;
    if sample_index >= samples.n_samples {
        return;
    }
    let pixel_pos = vec2i(0);
    let pixel_posf = vec2f(pixel_pos);
    let graph_input = samples.samples[sample_index];
    main_graph(graph_input, pixel_pos, pixel_posf);
    let affected_tiles = package_render_math_IRect(affected_pixels_precise.min / i32(package_image__1image_tiling__1TILE_SIZE), (affected_pixels_precise.max - 1) / i32(package_image__1image_tiling__1TILE_SIZE) + 1);
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
    if all(id == vec3u(0u)) {
        let max_affected_tiles_count = vec2u(atomicLoad(&stroke_info.max_affected_tiles_count_x), atomicLoad(&stroke_info.max_affected_tiles_count_y));
        dab_infos.n_dabs = samples.n_samples;
        tile_allocation_dispatch = vec3u(package_render_math__2unsigned_div_ceil(max_affected_tiles_count.x, package_brush__1brush_types__3TILE_ALLOCATION_WORKGROUP_SIZE.x), package_render_math__2unsigned_div_ceil(max_affected_tiles_count.y, package_brush__1brush_types__3TILE_ALLOCATION_WORKGROUP_SIZE.y), package_render_math__2unsigned_div_ceil(samples.n_samples, package_brush__1brush_types__3TILE_ALLOCATION_WORKGROUP_SIZE.z));
        let max_affected_pixels_count = max_affected_tiles_count * vec2u(package_image__1image_tiling__1TILE_SIZE);
        main_dispatch = vec3u(package_render_math__2unsigned_div_ceil(max_affected_pixels_count.x, package_brush__1brush_types__2MAIN_WORKGROUP_SIZE.x), package_render_math__2unsigned_div_ceil(max_affected_pixels_count.y, package_brush__1brush_types__2MAIN_WORKGROUP_SIZE.y), package_render_math__2unsigned_div_ceil(samples.n_samples, package_brush__1brush_types__2MAIN_WORKGROUP_SIZE.z));
    }
}

struct package_render_math_Rect {
    min: vec2f,
    max: vec2f
}

struct package_render_math_IRect {
    min: vec2i,
    max: vec2i
}

fn package_render_math__1inverse_mat3x3(matrix: mat3x3<f32>) -> mat3x3<f32> {
    let tmp0 = cross(matrix[1], matrix[2]);
    let tmp1 = cross(matrix[2], matrix[0]);
    let tmp2 = cross(matrix[0], matrix[1]);
    let inv_det = 1.0 / dot(matrix[2], tmp2);
    return transpose(mat3x3<f32>(tmp0 * inv_det, tmp1 * inv_det, tmp2 * inv_det));
}

fn package_render_math__1transform_rect(rect: package_render_math_Rect, mat: mat3x3f) -> package_render_math_Rect {
    let p0 = mat * vec3f(rect.min, 1.0);
    let p1 = mat * vec3f(rect.max.x, rect.min.y, 1.0);
    let p2 = mat * vec3f(rect.min.x, rect.max.y, 1.0);
    let p3 = mat * vec3f(rect.max, 1.0);
    let min = min(min(p0.xy, p1.xy), min(p2.xy, p3.xy));
    let max = max(max(p0.xy, p1.xy), max(p2.xy, p3.xy));
    return package_render_math_Rect(min, max);
}

fn package_render_math__2unsigned_div_ceil(lhs: u32, rhs: u32) -> u32 {
    return (lhs + rhs - 1u) / rhs;
}

const package_brush__1brush_types__3TILE_ALLOCATION_WORKGROUP_SIZE = vec3u(1, 1, 8);

const package_brush__1brush_types__2MAIN_WORKGROUP_SIZE = vec3u(16, 16, 1);

struct package_brush__1brush_types_StrokeInfo {
    accumulated_bound_min_x: atomic<i32>,
    accumulated_bound_min_y: atomic<i32>,
    accumulated_bound_max_x: atomic<i32>,
    accumulated_bound_max_y: atomic<i32>,
    max_affected_tiles_count_x: atomic<u32>,
    max_affected_tiles_count_y: atomic<u32>,
    total_dabs: u32,
    _padding: u32
}

struct package_brush__1brush_types_OutputSamples {
    n_samples: u32,
    samples: array<package_brush__1brush_types_ComputedPenInput>
}

struct package_brush__1brush_types_DabInfos {
    n_dabs: u32,
    buf: array<package_brush__1brush_types_DabInfo>
}

struct package_brush__1brush_types_DabInfo {
    bound_min: vec2i,
    bound_max: vec2i
}

struct package_brush__1brush_types_ComputedPenInput {
    position: vec2f
}

const package_image__1blend_modes__1BLEND_EPSILON: f32 = 1e-6;

fn package_image__1blend_modes__1clamp01_rgb(color: vec3f) -> vec3f {
    return clamp(color, vec3f(0.0), vec3f(1.0));
}

fn package_image__1blend_modes__1blend_compose(src: vec4f, dst: vec4f, blended_rgb: vec3f) -> vec4f {
    let src_a = clamp(src.a, 0.0, 1.0);
    let dst_a = clamp(dst.a, 0.0, 1.0);
    let out_a = src_a + dst_a * (1.0 - src_a);
    if out_a <= package_image__1blend_modes__1BLEND_EPSILON {
        return vec4f(0.0);
    }
    let out_rgb_premul = src.rgb * src_a * (1.0 - dst_a) + dst.rgb * dst_a * (1.0 - src_a) + blended_rgb * src_a * dst_a;
    let out_rgb = out_rgb_premul / out_a;
    return vec4f(package_image__1blend_modes__1clamp01_rgb(out_rgb), out_a);
}

fn package_image__1blend_modes__1blend_normal(src: vec4f, dst: vec4f) -> vec4f {
    return package_image__1blend_modes__1blend_compose(src, dst, src.rgb);
}

const package_image__1image_tiling__1TILE_SIZE: u32 = 256;


-------------- Stroke postprocess graph shader --------------
-------------- Shader --------------
@group(0) @binding(1)
var<storage, read_write> stroke_info: package_brush__1brush_types_StrokeInfo;

@group(0) @binding(3)
var<storage> target_layer_tile_info: array<package_image__1image_tiling_TileInfo>;

@group(0) @binding(4)
var target_layer: texture_storage_2d_array<r32uint, read>;

@group(0) @binding(5)
var<storage> buffer_tile_info: package_brush__1brush_types_DynamicTileInfo;

@group(0) @binding(6)
var stroke_buffer_a: texture_storage_2d_array<r32uint, read_write>;

@group(0) @binding(7)
var stroke_buffer_b: texture_storage_2d_array<r32uint, read_write>;

@group(0) @binding(8)
var<storage, read_write> dab_infos: package_brush__1brush_types_DabInfos;

@group(0) @binding(9)
var<storage, read_write> fence: package_brush__1brush_types_PassFence;

@group(0) @binding(32)
var<storage> external_Opacity_96919cd9_8758_46da_bc0c_8ec225a95152: f32;

fn main_graph(graph_input: package_brush__1brush_types_ComputedPenInput, pixel_pos: vec2i, pixel_posf: vec2f) {
    wait_for_sample(0);
    let output_0_ = pixel_posf;
    let output_1_ = current_input_color(vec2i(output_0_));
    let output_2_ = external_Opacity_96919cd9_8758_46da_bc0c_8ec225a95152;
    let output_3_ = output_1_.r;
    let output_4_ = output_1_.g;
    let output_5_ = output_1_.b;
    let output_6_ = output_1_.a;
    let output_7_ = output_6_ * output_2_;
    let output_8_ = target_layer_color(vec2i(output_0_));
    let output_9_ = vec4f(output_3_, output_4_, output_5_, output_7_);
    let output_10_ = get_accumulated_pixel_bounds();
    let output_11_ = vec2f(output_10_.min);
    let output_12_ = vec2f(output_10_.max);
    let output_13_ = package_image__1blend_modes__1blend_normal(output_9_, output_8_);
    let output_14_ = filter_within_bounds(pixel_pos, output_13_, output_11_, output_12_);
    set_output_color(pixel_pos, output_14_);
    finish_sample_thread();
    storageBarrier();
}

fn get_accumulated_pixel_bounds() -> package_render_math_IRect {
    {
        return package_render_math_IRect(stroke_info.accumulated_bound_min * i32(package_image__1image_tiling__1TILE_SIZE), stroke_info.accumulated_bound_max * i32(package_image__1image_tiling__1TILE_SIZE) + 1);
    }
}

fn is_current_input_buffer_a() -> bool {
    {
        return (atomicLoad(&fence.cur_sample) + stroke_info.total_dabs) % 2u == 0;
    }
}

fn convert_pixel_to_layer_tile(pixel: vec2i) -> vec3u {
    for (var i = 0u; i < arrayLength(&target_layer_tile_info); i = i + 1u) {
        let info = target_layer_tile_info[i];
        if pixel.x >= info.origin.x && pixel.x < info.origin.x + i32(package_image__1image_tiling__1TILE_SIZE) && pixel.y >= info.origin.y && pixel.y < info.origin.y + i32(package_image__1image_tiling__1TILE_SIZE) {
            return vec3u(vec2u(pixel - info.origin), i);
        }
    }
    return vec3u(4294967295u, 0, 0);
}

fn convert_pixel_to_buffer_tile(pixel: vec2i) -> vec3u {
    for (var i = 0u; i < buffer_tile_info.n_tiles; i = i + 1u) {
        let info = buffer_tile_info.buf[i];
        if pixel.x >= info.origin.x && pixel.x < info.origin.x + i32(package_image__1image_tiling__1TILE_SIZE) && pixel.y >= info.origin.y && pixel.y < info.origin.y + i32(package_image__1image_tiling__1TILE_SIZE) {
            return vec3u(vec2u(pixel - info.origin), i);
        }
    }
    return vec3u(4294967295u, 0, 0);
}

fn current_input_color(pixel_pos: vec2i) -> vec4f {
    {
        let tile_index = convert_pixel_to_buffer_tile(pixel_pos);
        if tile_index.z == 4294967295u {
            return vec4f(0.0);
        }
        let color = select(textureLoad(stroke_buffer_b, vec2i(tile_index.xy), tile_index.z), textureLoad(stroke_buffer_a, vec2i(tile_index.xy), tile_index.z), is_current_input_buffer_a());
        return package_image__1texture_unpack__2unpack_rgba8_texel(color);
    }
}

fn target_layer_color(pixel_pos: vec2i) -> vec4f {
    let tile_index = convert_pixel_to_layer_tile(pixel_pos);
    if tile_index.z == 4294967295u {
        return vec4f(0.0);
    }
    return package_image__1texture_unpack__2unpack_rgba8_texel(textureLoad(target_layer, vec2i(tile_index.xy), tile_index.z));
}

fn filter_within_bounds(pixel: vec2i, color: vec4f, bounds_min: vec2f, bounds_max: vec2f) -> vec4f {
    let pixelf = vec2f(pixel);
    if pixelf.x < bounds_min.x || pixelf.x >= bounds_max.x || pixelf.y < bounds_min.y || pixelf.y >= bounds_max.y {
        return vec4f(0.0);
    }
    return color;
}

fn set_output_color(pixel_pos: vec2i, color: vec4f) {
    {
        let tile_index = convert_pixel_to_buffer_tile(pixel_pos);
        if tile_index.z == 4294967295u {
            return;
        }
        let packed = package_image__1texture_unpack__2pack_rgba8_texel(color);
        if is_current_input_buffer_a() {
            textureStore(stroke_buffer_b, vec2i(tile_index.xy), tile_index.z, packed);
        }
        else {
            textureStore(stroke_buffer_a, vec2i(tile_index.xy), tile_index.z, packed);
        }
    }
}

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
        let expected_threads = vec2u(waiting_for_dab.bound_max - waiting_for_dab.bound_min) * package_image__1image_tiling__1TILE_SIZE;
        if atomicCompareExchangeWeak(&fence.cur_sample_finished_threads, expected_threads.x * expected_threads.y, 0u).exchanged {
            atomicStore(&fence.cur_sample, expected_sample_index);
            return;
        }
        timeout -= 1u;
        if timeout == 0u {
            atomicStore(&fence.cur_sample_finished_threads, 0u);
            atomicStore(&fence.cur_sample, expected_sample_index);
            return;
        }
    }
}

fn finish_sample_thread() {
    atomicAdd(&fence.cur_sample_finished_threads, 1u);
}

@compute @workgroup_size(16, 16, 1)
fn main(@builtin(global_invocation_id) id: vec3u) {
    let bounds = get_accumulated_pixel_bounds();
    let pixel_pos = vec2i(id.xy) + bounds.min;
    if any(pixel_pos >= bounds.max) {
        return;
    }
    let pixel_posf = vec2f(pixel_pos);
    let dummy_input = package_brush__1brush_types_ComputedPenInput();
    main_graph(dummy_input, pixel_pos, pixel_posf);
}

struct package_render_math_IRect {
    min: vec2i,
    max: vec2i
}

struct package_brush__1brush_types_StrokeInfo {
    accumulated_bound_min: vec2i,
    accumulated_bound_max: vec2i,
    max_affected_tiles_count: vec2u,
    total_dabs: u32,
    _padding: u32
}

struct package_brush__1brush_types_DynamicTileInfo {
    n_tiles: u32,
    buf: array<package_image__1image_tiling_TileInfo>
}

struct package_brush__1brush_types_DabInfos {
    n_dabs: u32,
    buf: array<package_brush__1brush_types_DabInfo>
}

struct package_brush__1brush_types_DabInfo {
    bound_min: vec2i,
    bound_max: vec2i
}

struct package_brush__1brush_types_ComputedPenInput {
    position: vec2f
}

struct package_brush__1brush_types_PassFence {
    cur_sample: atomic<u32>,
    cur_sample_finished_threads: atomic<u32>
}

const package_image__1image_tiling__1TILE_SIZE: u32 = 256;

struct package_image__1image_tiling_TileInfo {
    index: vec2i,
    origin: vec2i
}

fn package_image__1texture_unpack__2unpack_rgba8_texel(texel: vec4u) -> vec4f {
    return package_image__1texture_unpack__1unpack_rgba8(texel.r);
}

fn package_image__1texture_unpack__2pack_rgba8_texel(color: vec4f) -> vec4u {
    return vec4u(package_image__1texture_unpack__1pack_rgba8(color), 0, 0, 0);
}

fn package_image__1texture_unpack__1unpack_rgba8(x: u32) -> vec4f {
    let r = f32((x >> 0) & 255) / 255.0;
    let g = f32((x >> 8) & 255) / 255.0;
    let b = f32((x >> 16) & 255) / 255.0;
    let a = f32((x >> 24) & 255) / 255.0;
    return vec4f(r, g, b, a);
}

fn package_image__1texture_unpack__1pack_rgba8(color: vec4f) -> u32 {
    let r = u32(clamp(color.r * 255.0, 0.0, 255.0));
    let g = u32(clamp(color.g * 255.0, 0.0, 255.0));
    let b = u32(clamp(color.b * 255.0, 0.0, 255.0));
    let a = u32(clamp(color.a * 255.0, 0.0, 255.0));
    return (a << 24) | (b << 16) | (g << 8) | r;
}

const package_image__1blend_modes__1BLEND_EPSILON: f32 = 1e-6;

fn package_image__1blend_modes__1clamp01_rgb(color: vec3f) -> vec3f {
    return clamp(color, vec3f(0.0), vec3f(1.0));
}

fn package_image__1blend_modes__1blend_compose(src: vec4f, dst: vec4f, blended_rgb: vec3f) -> vec4f {
    let src_a = clamp(src.a, 0.0, 1.0);
    let dst_a = clamp(dst.a, 0.0, 1.0);
    let out_a = src_a + dst_a * (1.0 - src_a);
    if out_a <= package_image__1blend_modes__1BLEND_EPSILON {
        return vec4f(0.0);
    }
    let out_rgb_premul = src.rgb * src_a * (1.0 - dst_a) + dst.rgb * dst_a * (1.0 - src_a) + blended_rgb * src_a * dst_a;
    let out_rgb = out_rgb_premul / out_a;
    return vec4f(package_image__1blend_modes__1clamp01_rgb(out_rgb), out_a);
}

fn package_image__1blend_modes__1blend_normal(src: vec4f, dst: vec4f) -> vec4f {
    return package_image__1blend_modes__1blend_compose(src, dst, src.rgb);
}

-------------- Size estimation --------------
@group(0) @binding(1)
var<storage, read_write> stroke_info: package_brush__1brush_types_StrokeInfo;

@group(0) @binding(3)
var<storage> target_layer_tile_info: array<package_image__1image_tiling_TileInfo>;

@group(0) @binding(4)
var target_layer: texture_storage_2d_array<r32uint, read>;

@group(0) @binding(16)
var<storage, read_write> tile_allocation_dispatch: vec3u;

@group(0) @binding(17)
var<storage, read_write> main_dispatch: vec3u;

@group(0) @binding(32)
var<storage> external_Opacity_96919cd9_8758_46da_bc0c_8ec225a95152: f32;

fn main_graph(graph_input: package_brush__1brush_types_ComputedPenInput, pixel_pos: vec2i, pixel_posf: vec2f) {
    let output_0_ = pixel_posf;
    let output_1_ = current_input_color(vec2i(output_0_));
    let output_2_ = external_Opacity_96919cd9_8758_46da_bc0c_8ec225a95152;
    let output_3_ = output_1_.r;
    let output_4_ = output_1_.g;
    let output_5_ = output_1_.b;
    let output_6_ = output_1_.a;
    let output_7_ = output_6_ * output_2_;
    let output_8_ = target_layer_color(vec2i(output_0_));
    let output_9_ = vec4f(output_3_, output_4_, output_5_, output_7_);
    let output_10_ = get_accumulated_pixel_bounds();
    let output_11_ = vec2f(output_10_.min);
    let output_12_ = vec2f(output_10_.max);
    let output_13_ = package_image__1blend_modes__1blend_normal(output_9_, output_8_);
    let output_14_ = filter_within_bounds(pixel_pos, output_13_, output_11_, output_12_);
    set_output_color(pixel_pos, output_14_);
}

fn get_accumulated_pixel_bounds() -> package_render_math_IRect {
    {
        return package_render_math_IRect(vec2i(atomicLoad(&stroke_info.accumulated_bound_min_x), atomicLoad(&stroke_info.accumulated_bound_min_y)) * i32(package_image__1image_tiling__1TILE_SIZE), vec2i(atomicLoad(&stroke_info.accumulated_bound_max_x), atomicLoad(&stroke_info.accumulated_bound_max_y)) * i32(package_image__1image_tiling__1TILE_SIZE) + 1);
    }
}

fn convert_pixel_to_layer_tile(pixel: vec2i) -> vec3u {
    for (var i = 0u; i < arrayLength(&target_layer_tile_info); i = i + 1u) {
        let info = target_layer_tile_info[i];
        if pixel.x >= info.origin.x && pixel.x < info.origin.x + i32(package_image__1image_tiling__1TILE_SIZE) && pixel.y >= info.origin.y && pixel.y < info.origin.y + i32(package_image__1image_tiling__1TILE_SIZE) {
            return vec3u(vec2u(pixel - info.origin), i);
        }
    }
    return vec3u(4294967295u, 0, 0);
}

fn current_input_color(pixel_pos: vec2i) -> vec4f {
    {
        return vec4f(0.0);
    }
}

fn target_layer_color(pixel_pos: vec2i) -> vec4f {
    let tile_index = convert_pixel_to_layer_tile(pixel_pos);
    if tile_index.z == 4294967295u {
        return vec4f(0.0);
    }
    return package_image__1texture_unpack__2unpack_rgba8_texel(textureLoad(target_layer, vec2i(tile_index.xy), tile_index.z));
}

fn filter_within_bounds(pixel: vec2i, color: vec4f, bounds_min: vec2f, bounds_max: vec2f) -> vec4f {
    {
        require_bounds(vec2i(bounds_min), vec2i(bounds_max));
    }
    let pixelf = vec2f(pixel);
    if pixelf.x < bounds_min.x || pixelf.x >= bounds_max.x || pixelf.y < bounds_min.y || pixelf.y >= bounds_max.y {
        return vec4f(0.0);
    }
    return color;
}

fn set_output_color(pixel_pos: vec2i, color: vec4f) {

}

var<private> affected_pixels_precise: package_render_math_IRect;

fn require_bounds(pixel_min: vec2i, pixel_max: vec2i) {
    affected_pixels_precise.min = min(affected_pixels_precise.min, pixel_min);
    affected_pixels_precise.max = max(affected_pixels_precise.max, pixel_max);
}

@compute @workgroup_size(1, 1, 1)
fn estimate() {
    affected_pixels_precise = package_render_math_IRect(vec2i(2147483647), vec2i(-2147483647));
    let pixel_pos = vec2i(0);
    let pixel_posf = vec2f(pixel_pos);
    let dummy_input = package_brush__1brush_types_ComputedPenInput();
    main_graph(dummy_input, pixel_pos, pixel_posf);
    let accu_min_x = atomicLoad(&stroke_info.accumulated_bound_min_x);
    let accu_min_y = atomicLoad(&stroke_info.accumulated_bound_min_y);
    let accu_max_x = atomicLoad(&stroke_info.accumulated_bound_max_x);
    let accu_max_y = atomicLoad(&stroke_info.accumulated_bound_max_y);
    let affected_tiles = package_render_math_IRect(min(affected_pixels_precise.min / i32(package_image__1image_tiling__1TILE_SIZE), vec2i(accu_min_x, accu_min_y)), max((affected_pixels_precise.max - 1) / i32(package_image__1image_tiling__1TILE_SIZE) + 1, vec2i(accu_max_x, accu_max_y)));
    let affected_tiles_count = vec2u(affected_tiles.max - affected_tiles.min);
    tile_allocation_dispatch = vec3u(package_render_math__2unsigned_div_ceil(affected_tiles_count.x, package_brush__1brush_types__3TILE_ALLOCATION_WORKGROUP_SIZE.x), package_render_math__2unsigned_div_ceil(affected_tiles_count.y, package_brush__1brush_types__3TILE_ALLOCATION_WORKGROUP_SIZE.y), 1u);
    let affected_pixels_count = affected_tiles_count * vec2u(package_image__1image_tiling__1TILE_SIZE);
    main_dispatch = vec3u(package_render_math__2unsigned_div_ceil(affected_pixels_count.x, package_brush__1brush_types__2MAIN_WORKGROUP_SIZE.x), package_render_math__2unsigned_div_ceil(affected_pixels_count.y, package_brush__1brush_types__2MAIN_WORKGROUP_SIZE.y), 1u);
}

struct package_render_math_IRect {
    min: vec2i,
    max: vec2i
}

fn package_render_math__2unsigned_div_ceil(lhs: u32, rhs: u32) -> u32 {
    return (lhs + rhs - 1u) / rhs;
}

const package_brush__1brush_types__3TILE_ALLOCATION_WORKGROUP_SIZE = vec3u(1, 1, 8);

const package_brush__1brush_types__2MAIN_WORKGROUP_SIZE = vec3u(16, 16, 1);

struct package_brush__1brush_types_StrokeInfo {
    accumulated_bound_min_x: atomic<i32>,
    accumulated_bound_min_y: atomic<i32>,
    accumulated_bound_max_x: atomic<i32>,
    accumulated_bound_max_y: atomic<i32>,
    max_affected_tiles_count_x: atomic<u32>,
    max_affected_tiles_count_y: atomic<u32>,
    total_dabs: u32,
    _padding: u32
}

struct package_brush__1brush_types_ComputedPenInput {
    position: vec2f
}

const package_image__1image_tiling__1TILE_SIZE: u32 = 256;

struct package_image__1image_tiling_TileInfo {
    index: vec2i,
    origin: vec2i
}

fn package_image__1texture_unpack__2unpack_rgba8_texel(texel: vec4u) -> vec4f {
    return package_image__1texture_unpack__1unpack_rgba8(texel.r);
}

fn package_image__1texture_unpack__1unpack_rgba8(x: u32) -> vec4f {
    let r = f32((x >> 0) & 255) / 255.0;
    let g = f32((x >> 8) & 255) / 255.0;
    let b = f32((x >> 16) & 255) / 255.0;
    let a = f32((x >> 24) & 255) / 255.0;
    return vec4f(r, g, b, a);
}

const package_image__1blend_modes__1BLEND_EPSILON: f32 = 1e-6;

fn package_image__1blend_modes__1clamp01_rgb(color: vec3f) -> vec3f {
    return clamp(color, vec3f(0.0), vec3f(1.0));
}

fn package_image__1blend_modes__1blend_compose(src: vec4f, dst: vec4f, blended_rgb: vec3f) -> vec4f {
    let src_a = clamp(src.a, 0.0, 1.0);
    let dst_a = clamp(dst.a, 0.0, 1.0);
    let out_a = src_a + dst_a * (1.0 - src_a);
    if out_a <= package_image__1blend_modes__1BLEND_EPSILON {
        return vec4f(0.0);
    }
    let out_rgb_premul = src.rgb * src_a * (1.0 - dst_a) + dst.rgb * dst_a * (1.0 - src_a) + blended_rgb * src_a * dst_a;
    let out_rgb = out_rgb_premul / out_a;
    return vec4f(package_image__1blend_modes__1clamp01_rgb(out_rgb), out_a);
}

fn package_image__1blend_modes__1blend_normal(src: vec4f, dst: vec4f) -> vec4f {
    return package_image__1blend_modes__1blend_compose(src, dst, src.rgb);
}


-------------- Texture usages --------------
  - 00000000-0000-0000-0000-000000000000
  - 01f79456-36ec-5de8-1673-2e32662df931
