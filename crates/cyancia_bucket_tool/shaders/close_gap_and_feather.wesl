import package::types::BucketParams;
import image::image_tiling::{TileInfo, TILE_SIZE};

const WORKGROUP_SIZE = 16u;
const MASK_OFF = 0.0;
const MASK_ON = 1.0;
const NO_TILE = -1;
const INVALID_SEED = vec2f(-1.0, -1.0);

struct JumpParams {
    jump: u32,
}

@group(0) @binding(0) var<uniform> params: BucketParams;
@group(0) @binding(1) var mask: texture_storage_2d_array<r8unorm, read_write>;
@group(0) @binding(2) var seed_src: texture_storage_2d_array<rg32float, read>;
@group(0) @binding(3) var seed_dst: texture_storage_2d_array<rg32float, write>;
@group(0) @binding(4) var<storage> tile_info: array<TileInfo>;
@group(0) @binding(5) var<uniform> jump_params: JumpParams;

fn is_outside_image(coord: vec2i) -> bool {
    return params.transparent_mode != 0u &&
        (coord.x < 0 || coord.y < 0 ||
         coord.x >= i32(params.image_size.x) || coord.y >= i32(params.image_size.y));
}

fn find_tile_for_coord(coord: vec2i) -> i32 {
    for (var i = 0u; i < arrayLength(&tile_info); i = i + 1u) {
        let info = tile_info[i];
        if coord.x >= info.origin.x && coord.x < info.origin.x + i32(TILE_SIZE) &&
           coord.y >= info.origin.y && coord.y < info.origin.y + i32(TILE_SIZE) {
            return i32(i);
        }
    }
    return NO_TILE;
}

fn load_mask_global(coord: vec2i) -> f32 {
    if is_outside_image(coord) {
        return MASK_OFF;
    }

    let tile_index = find_tile_for_coord(coord);
    if tile_index == NO_TILE {
        return MASK_OFF;
    }

    let info = tile_info[u32(tile_index)];
    let local_coord = vec2u(coord - info.origin);
    return textureLoad(mask, local_coord, tile_index).r;
}

fn load_seed_global(coord: vec2i) -> vec2f {
    let tile_index = find_tile_for_coord(coord);
    if tile_index == NO_TILE {
        return INVALID_SEED;
    }

    let info = tile_info[u32(tile_index)];
    let local_coord = vec2u(coord - info.origin);
    return textureLoad(seed_src, local_coord, u32(tile_index)).xy;
}

fn is_valid_seed(seed: vec2f) -> bool {
    return seed.x >= 0.0 && seed.y >= 0.0;
}

fn is_edge(global_coord: vec2i, center: f32) -> bool {
    return abs(center - load_mask_global(global_coord + vec2i(-1, 0))) > 0.5 ||
        abs(center - load_mask_global(global_coord + vec2i(1, 0))) > 0.5 ||
        abs(center - load_mask_global(global_coord + vec2i(0, -1))) > 0.5 ||
        abs(center - load_mask_global(global_coord + vec2i(0, 1))) > 0.5;
}

@compute
@workgroup_size(WORKGROUP_SIZE, WORKGROUP_SIZE, 1)
fn seed_edges(@builtin(global_invocation_id) global_id: vec3u) {
    let tile_index = global_id.z;
    let local_coord = global_id.xy;
    if tile_index >= arrayLength(&tile_info) || any(local_coord >= vec2u(TILE_SIZE)) {
        return;
    }

    let tile = tile_info[tile_index];
    let global_coord = tile.origin + vec2i(local_coord);
    let center = load_mask_global(global_coord);
    let seed = select(INVALID_SEED, vec2f(global_coord), center > 0.5 && is_edge(global_coord, center));
    textureStore(seed_dst, local_coord, tile_index, vec4f(seed, 0.0, 0.0));
}

@compute
@workgroup_size(WORKGROUP_SIZE, WORKGROUP_SIZE, 1)
fn jfa_jump(@builtin(global_invocation_id) global_id: vec3u) {
    let tile_index = global_id.z;
    let local_coord = global_id.xy;
    if tile_index >= arrayLength(&tile_info) || any(local_coord >= vec2u(TILE_SIZE)) {
        return;
    }

    let tile = tile_info[tile_index];
    let global_coord = tile.origin + vec2i(local_coord);
    let current_seed = load_seed_global(global_coord);
    var best_seed = current_seed;
    var best_distance = select(3.402823e38, distance(vec2f(global_coord), current_seed), is_valid_seed(current_seed));
    let jump = i32(jump_params.jump);

    for (var y = -1; y <= 1; y = y + 1) {
        for (var x = -1; x <= 1; x = x + 1) {
            let candidate_seed = load_seed_global(global_coord + vec2i(x * jump, y * jump));
            if is_valid_seed(candidate_seed) {
                let candidate_distance = distance(vec2f(global_coord), candidate_seed);
                if candidate_distance < best_distance {
                    best_distance = candidate_distance;
                    best_seed = candidate_seed;
                }
            }
        }
    }

    textureStore(seed_dst, local_coord, tile_index, vec4f(best_seed, 0.0, 0.0));
}

@compute
@workgroup_size(WORKGROUP_SIZE, WORKGROUP_SIZE, 1)
fn feather_resolve_alpha(@builtin(global_invocation_id) global_id: vec3u) {
    let tile_index = global_id.z;
    let local_coord = global_id.xy;
    if tile_index >= arrayLength(&tile_info) || any(local_coord >= vec2u(TILE_SIZE)) {
        return;
    }

    let tile = tile_info[tile_index];
    let global_coord = tile.origin + vec2i(local_coord);
    let center = load_mask_global(global_coord);
    if center <= 0.5 {
        return;
    }

    let seed = load_seed_global(global_coord);
    if !is_valid_seed(seed) {
        return;
    }

    let alpha = clamp(distance(vec2f(global_coord), seed) / f32(params.feather), MASK_OFF, MASK_ON);
    textureStore(mask, local_coord, tile_index, vec4f(alpha, 0.0, 0.0, 0.0));
}

@compute
@workgroup_size(WORKGROUP_SIZE, WORKGROUP_SIZE, 1)
fn close_gap_resolve(@builtin(global_invocation_id) global_id: vec3u) {
    let tile_index = global_id.z;
    let local_coord = global_id.xy;
    if tile_index >= arrayLength(&tile_info) || any(local_coord >= vec2u(TILE_SIZE)) {
        return;
    }

    let tile = tile_info[tile_index];
    let global_coord = tile.origin + vec2i(local_coord);
    let global_coordf = vec2f(global_coord);

    let seed = load_seed_global(global_coord);

    if !is_valid_seed(seed) {
        return;
    }

    let cur_dist = distance(global_coordf, seed);
    if cur_dist > f32(params.close_gap) {
        return;
    }

    let seed_dir = normalize(seed - global_coordf);
    let neighbor_dir = vec2f(-seed_dir.y, seed_dir.x);
    let left_coord = vec2i(global_coordf - neighbor_dir);
    let right_coord = vec2i(global_coordf + neighbor_dir);
    let left_seed = load_seed_global(left_coord);
    let right_seed = load_seed_global(right_coord);
    if !is_valid_seed(left_seed) || !is_valid_seed(right_seed) {
        return;
    }

    let left_dist = distance(vec2f(left_coord), left_seed);
    let right_dist = distance(vec2f(right_coord), right_seed);

    if cur_dist > left_dist && cur_dist > right_dist {
        var p0 = vec2i(left_seed);
        let p1 = vec2i(right_seed);
        let delta = abs(p1 - p0);
        let step = vec2i(select(-1, 1, p0.x < p1.x), select(-1, 1, p0.y < p1.y));
        var err = delta.x - delta.y;

        loop {
            let line_tile_index = find_tile_for_coord(p0);
            if line_tile_index != NO_TILE {
                let line_tile = tile_info[u32(line_tile_index)];
                let line_local_coord = vec2u(p0 - line_tile.origin);
                textureStore(mask, line_local_coord, u32(line_tile_index), vec4f(MASK_OFF, 0.0, 0.0, 0.0));
            }

            if all(p0 == p1) {
                break;
            }

            let err2 = err * 2;
            if err2 > -delta.y {
                err = err - delta.y;
                p0.x = p0.x + step.x;
            }
            if err2 < delta.x {
                err = err + delta.x;
                p0.y = p0.y + step.y;
            }
        }
    }
}
