import package::image::image_tiling::{TileInfo};

const MAX_SAMPLES_BETWEEN_INPUTS: u32 = 256;

const TILE_ALLOCATION_WORKGROUP_SIZE = vec3u(1, 1, 8);
const MAIN_WORKGROUP_SIZE = vec3u(16, 16, 1);
const ESTIMATION_WORKGROUP_SIZE = vec3u(1, 1, 8);

struct StrokeInfo {
    // In tiles
    accumulated_bound_min: vec2i,
    accumulated_bound_max: vec2i,

    total_dabs: u32,
}

struct DynamicTileInfo {
    n_tiles: u32,
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
