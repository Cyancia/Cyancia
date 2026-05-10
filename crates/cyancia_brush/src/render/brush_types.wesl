import package::image::image_tiling::{TileInfo};
import package::render::math::{IRect};

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
    time: Time,
}

struct StrokePostprocessData {
    accumulated_bound: IRect,
    time: Time,
}

struct Time {
    now: f32,
    stroke_begin: f32,
}
