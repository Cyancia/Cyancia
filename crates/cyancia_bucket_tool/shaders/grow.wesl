import package::types::{BucketParams};
import image::image_tiling::{TileInfo, TILE_SIZE};
import render::math::{circle_rectangle_intersect};

const WORKGROUP_SIZE = 16u;
const MASK_OFF = 0.0;
const MASK_ON = 1.0;
const MASK_THRESHOLD = 0.5;
const NO_TILE = -1;
const ACTIVE_TILE_BIT = 1u << 8u;

const OFFSETS: array<vec2i, 8> = array<vec2i, 8>(
    vec2i(-1, -1),
    vec2i(0, -1),
    vec2i(1, -1),
    vec2i(1, 0),
    vec2i(1, 1),
    vec2i(0, 1),
    vec2i(-1, 1),
    vec2i(-1, 0)
);

@group(0) @binding(0) var<uniform> params: BucketParams;
@group(0) @binding(1) var source_mask: texture_storage_2d_array<r32float, read>;
@group(0) @binding(2) var destination_mask: texture_storage_2d_array<r32float, write>;
@group(0) @binding(3) var<storage> source_tile_info: array<TileInfo>;
@group(0) @binding(4) var<storage> destination_tile_info: array<TileInfo>;
// (allocate_info[i] & (1 << j)) != 0 means the tile i should grow in the j-th direction
@group(0) @binding(5) var<storage, read_write> allocate_info: array<atomic<u32>>;

fn is_outside_image(coord: vec2i) -> bool {
    return params.transparent_mode != 0u &&
        (coord.x < 0 || coord.y < 0 ||
         coord.x >= i32(params.image_size.x) || coord.y >= i32(params.image_size.y));
}

fn is_tile_outside_image(tile_coord: vec2i) -> bool {
    let tile_min = tile_coord * vec2i(i32(TILE_SIZE));
    let tile_max = tile_min + vec2i(i32(TILE_SIZE));
    return params.transparent_mode != 0u &&
        (tile_max.x <= 0 || tile_max.y <= 0 ||
         tile_min.x >= i32(params.image_size.x) || tile_min.y >= i32(params.image_size.y));
}

fn find_source_tile_for_coord(coord: vec2i) -> i32 {
    for (var i = 0u; i < arrayLength(&source_tile_info); i = i + 1u) {
        let info = source_tile_info[i];
        if coord.x >= info.origin.x && coord.x < info.origin.x + i32(TILE_SIZE) &&
           coord.y >= info.origin.y && coord.y < info.origin.y + i32(TILE_SIZE) {
            return i32(i);
        }
    }
    return NO_TILE;
}

fn load_source_mask_global(coord: vec2i) -> f32 {
    if is_outside_image(coord) {
        return MASK_OFF;
    }

    let tile_index = find_source_tile_for_coord(coord);
    if tile_index == NO_TILE {
        return MASK_OFF;
    }

    let info = source_tile_info[u32(tile_index)];
    let local_coord = vec2u(coord - info.origin);
    return textureLoad(source_mask, local_coord, u32(tile_index)).x;
}

fn squared_distance_to_tile(local_coord: vec2i, neighbor_offset: vec2i) -> i32 {
    let neighbor_min = neighbor_offset * i32(TILE_SIZE);
    let neighbor_max = neighbor_min + vec2i(i32(TILE_SIZE) - 1);

    var dx = 0;
    if local_coord.x < neighbor_min.x {
        dx = neighbor_min.x - local_coord.x;
    } else if local_coord.x > neighbor_max.x {
        dx = local_coord.x - neighbor_max.x;
    }

    var dy = 0;
    if local_coord.y < neighbor_min.y {
        dy = neighbor_min.y - local_coord.y;
    } else if local_coord.y > neighbor_max.y {
        dy = local_coord.y - neighbor_max.y;
    }

    return dx * dx + dy * dy;
}

fn row_disk_x_limit(dy: i32, radius: i32, radius_squared: i32) -> i32 {
    var x_limit = radius;
    let y_squared = dy * dy;
    while x_limit * x_limit + y_squared > radius_squared {
        x_limit = x_limit - 1;
    }
    return x_limit;
}

fn dilate_pixel(global_coord: vec2i, radius: i32) -> f32 {
    let radius_squared = radius * radius;
    var found = false;

    for (var dy = -radius; dy <= radius && !found; dy = dy + 1) {
        let x_limit = row_disk_x_limit(dy, radius, radius_squared);
        for (var dx = -x_limit; dx <= x_limit && !found; dx = dx + 1) {
            if dx * dx + dy * dy <= radius_squared &&
               load_source_mask_global(global_coord + vec2i(dx, dy)) > MASK_THRESHOLD {
                found = true;
            }
        }
    }

    return select(MASK_OFF, MASK_ON, found);
}

fn erode_pixel(global_coord: vec2i, center: f32, radius: i32) -> f32 {
    if center <= MASK_THRESHOLD {
        return MASK_OFF;
    }

    let radius_squared = radius * radius;
    var kept = true;

    for (var dy = -radius; dy <= radius && kept; dy = dy + 1) {
        let x_limit = row_disk_x_limit(dy, radius, radius_squared);
        for (var dx = -x_limit; dx <= x_limit && kept; dx = dx + 1) {
            if dx * dx + dy * dy <= radius_squared &&
               load_source_mask_global(global_coord + vec2i(dx, dy)) <= MASK_THRESHOLD {
                kept = false;
            }
        }
    }

    return select(MASK_OFF, MASK_ON, kept);
}

@compute
@workgroup_size(16, 16, 1)
fn estimate(@builtin(global_invocation_id) global_id: vec3u) {
    let tile_index = global_id.z;
    let local_coord = global_id.xy;
    if tile_index >= arrayLength(&source_tile_info) || any(local_coord >= vec2u(TILE_SIZE)) {
        return;
    }

    let global_coord = source_tile_info[tile_index].origin + vec2i(local_coord);
    if is_outside_image(global_coord) {
        return;
    }

    let mask_value = textureLoad(source_mask, local_coord, u32(tile_index)).x;
    if mask_value < MASK_THRESHOLD {
        return;
    }

    atomicOr(&allocate_info[tile_index], ACTIVE_TILE_BIT);

    if params.grow <= 0 {
        return;
    }

    for (var i = 0u; i < 8u; i += 1u) {
        let tile_coord = source_tile_info[tile_index].index + OFFSETS[i];
        if is_tile_outside_image(tile_coord) {
            continue;
        }

        let tile_min = tile_coord * vec2i(TILE_SIZE);
        let tile_max = tile_min + vec2i(TILE_SIZE);
        if !circle_rectangle_intersect(vec2f(global_coord), f32(params.grow + 1), vec2f(tile_min), vec2f(tile_max)) {
            // Not intersected, won't affect
            continue;
        }

        atomicOr(&allocate_info[tile_index], 1u << i);
    }
}

@compute
@workgroup_size(WORKGROUP_SIZE, WORKGROUP_SIZE, 1)
fn main(@builtin(global_invocation_id) global_id: vec3u) {
    let tile_index = global_id.z;
    let local_coord = global_id.xy;
    if tile_index >= arrayLength(&destination_tile_info) || any(local_coord >= vec2u(TILE_SIZE)) {
        return;
    }

    let tile = destination_tile_info[tile_index];
    let global_coord = tile.origin + vec2i(local_coord);
    if is_outside_image(global_coord) {
        textureStore(destination_mask, local_coord, tile_index, vec4f(MASK_OFF, 0.0, 0.0, 0.0));
        return;
    }

    let center = load_source_mask_global(global_coord);
    let radius = abs(params.grow);

    var value = select(MASK_OFF, MASK_ON, center > MASK_THRESHOLD);
    if params.grow > 0 {
        value = dilate_pixel(global_coord, radius);
    } else if params.grow < 0 {
        value = erode_pixel(global_coord, center, radius);
    }

    textureStore(destination_mask, local_coord, tile_index, vec4f(value, 0.0, 0.0, 0.0));
}
