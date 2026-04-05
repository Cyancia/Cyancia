import package::image::image_tiling::{TileInfo};

const MAX_SAMPLES_BETWEEN_INPUTS: u32 = 256;

const TILE_ALLOCATION_WORKGROUP_SIZE = vec3u(1, 1, 8);
const MAIN_WORKGROUP_SIZE = vec3u(16, 16, 1);
const ESTIMATION_WORKGROUP_SIZE = vec3u(1, 1, 8);

struct StrokeInfo {
    // In tiles
    @if(SIZE_ESTIMATION) accumulated_bound_min_x: atomic<i32>,
    @if(SIZE_ESTIMATION) accumulated_bound_min_y: atomic<i32>,
    @if(SIZE_ESTIMATION) accumulated_bound_max_x: atomic<i32>,
    @if(SIZE_ESTIMATION) accumulated_bound_max_y: atomic<i32>,
    @if(!SIZE_ESTIMATION) accumulated_bound_min: vec2i,
    @if(!SIZE_ESTIMATION) accumulated_bound_max: vec2i,

    @if(SIZE_ESTIMATION) max_affected_tiles_count_x: atomic<u32>,
    @if(SIZE_ESTIMATION) max_affected_tiles_count_y: atomic<u32>,
    @if(!SIZE_ESTIMATION) max_affected_tiles_count: vec2u,

    total_dabs: u32,
    fence_sync_failures: u32,
}

struct DynamicTileInfo {
    @if(TILE_ALLOCATION) n_tiles: atomic<u32>,
    @if(!TILE_ALLOCATION) n_tiles: u32,
    buf: array<TileInfo>,
}

struct OutputSamples {
    n_samples: u32,
    samples: array<ComputedPenInput>,
}

struct DabInfos {
    n_dabs: u32,
    buf: array<DabInfo>,
}

struct DabInfo {
    // In tiles
    bound_min: vec2i,
    bound_max: vec2i,
}

struct ComputedPenInput {
    position: vec2f,
    draw_direction_vec: vec2f,
    draw_direction_angle: f32,
}

struct PenInputSampler {
    last_input: PenInput,
    last_sample: ComputedPenInput,
    has_last_sample: u32,
}

struct PenInput {
    position: vec2f,
    // Inner control points of the cubic Bezier from the previous position to
    // this one.  Computed on the CPU by BezierInputSampler.
    bezier_control_prev: vec2f,
    bezier_control_cur: vec2f,
}

struct PassFence {
    cur_sample: atomic<u32>,
    cur_sample_finished_threads: atomic<u32>,
}
