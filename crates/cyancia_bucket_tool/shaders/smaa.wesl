import image::image_tiling::{TileInfo, TILE_SIZE};

const WORKGROUP_SIZE = 16u;
const MASK_OFF = 0.0;
const MASK_ON = 1.0;
const MIN_MASK_VALUE = 0.0;
const MAX_MASK_VALUE = 1.0;
const NO_TILE = -1;
const HORIZONTAL_EDGE = vec2i(1, 0);
const VERTICAL_EDGE = vec2i(0, 1);
const DIAGONAL_UP_EDGE = vec2i(1, -1);
const DIAGONAL_DOWN_EDGE = vec2i(1, 1);

struct SmaaParams {
    blend_strength: f32,
    diagonal_weight: f32,
    corner_preserve_strength: f32,
    edge_search_steps: u32,
}

@group(0) @binding(0) var input: texture_storage_2d_array<r32float, read>;
@group(0) @binding(1) var output: texture_storage_2d_array<r32float, write>;
@group(0) @binding(2) var<storage> tile_info: array<TileInfo>;
@group(0) @binding(3) var<uniform> params: SmaaParams;

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

fn load_mask_global(coord: vec2i, fallback: f32) -> f32 {
    let tile_index = find_tile_for_coord(coord);
    if tile_index == NO_TILE {
        return fallback;
    }

    let info = tile_info[u32(tile_index)];
    let local_coord = vec2u(coord - info.origin);
    return textureLoad(input, local_coord, u32(tile_index)).x;
}

fn mask_diff(a: f32, b: f32) -> bool {
    return abs(a - b) > 0.5;
}

fn edge_search(coord: vec2i, center: f32, dir: vec2i) -> f32 {
    let steps = max(params.edge_search_steps, 1u);
    for (var i = 1u; i <= steps; i = i + 1u) {
        let sample = load_mask_global(coord + dir * i32(i), center);
        if mask_diff(center, sample) {
            return 1.0 / f32(i);
        }
    }
    return 0.0;
}

fn axis_weight(coord: vec2i, center: f32, dir: vec2i) -> f32 {
    let negative = edge_search(coord, center, -dir);
    let positive = edge_search(coord, center, dir);
    return max(negative, positive);
}

fn transition_weight(coord: vec2i, center: f32, dir: vec2i) -> f32 {
    let sample = load_mask_global(coord + dir, center);
    if !mask_diff(center, sample) {
        return 0.0;
    }

    let same_axis = axis_weight(coord, center, vec2i(dir.y, dir.x));
    let corner_preserve = clamp(params.corner_preserve_strength * same_axis, 0.0, 1.0);
    return 1.0 - corner_preserve;
}

@compute
@workgroup_size(WORKGROUP_SIZE, WORKGROUP_SIZE, 1)
fn main(@builtin(global_invocation_id) global_id: vec3u) {
    let tile_index = global_id.z;
    let local_coord = global_id.xy;
    if tile_index >= arrayLength(&tile_info) || any(local_coord >= vec2u(TILE_SIZE)) {
        return;
    }

    let tile = tile_info[tile_index];
    let global_coord = tile.origin + vec2i(local_coord);
    let center = textureLoad(input, local_coord, tile_index).x;

    let horizontal = max(
        transition_weight(global_coord, center, -HORIZONTAL_EDGE),
        transition_weight(global_coord, center, HORIZONTAL_EDGE),
    );
    let vertical = max(
        transition_weight(global_coord, center, -VERTICAL_EDGE),
        transition_weight(global_coord, center, VERTICAL_EDGE),
    );
    let diagonal = max(
        max(
            transition_weight(global_coord, center, DIAGONAL_UP_EDGE),
            transition_weight(global_coord, center, -DIAGONAL_UP_EDGE),
        ),
        max(
            transition_weight(global_coord, center, DIAGONAL_DOWN_EDGE),
            transition_weight(global_coord, center, -DIAGONAL_DOWN_EDGE),
        ),
    ) * params.diagonal_weight;

    let edge_amount = clamp(max(max(horizontal, vertical), diagonal), 0.0, 1.0);
    if edge_amount == 0.0 {
        textureStore(output, local_coord, tile_index, vec4f(center, 0.0, 0.0, 0.0));
        return;
    }

    let blend_target = select(MASK_ON, MASK_OFF, center > 0.5);
    let blend_amount = clamp(edge_amount * params.blend_strength, 0.0, 0.5);
    let smoothed = clamp(mix(center, blend_target, blend_amount), MIN_MASK_VALUE, MAX_MASK_VALUE);
    textureStore(output, local_coord, tile_index, vec4f(smoothed, 0.0, 0.0, 0.0));
}
