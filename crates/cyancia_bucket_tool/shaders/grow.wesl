import package::types::{BucketParams};
import image::image_tiling::{TileInfo, TILE_SIZE};
import render::math::{circle_rectangle_intersect};

const MASK_BARRIER = 0.0;
const MASK_FILLABLE = 1.0;
const NO_TILE = -1;
const ACTIVE_TILE_BIT = 1u << 8u;

@group(0) @binding(0) var<uniform> params: BucketParams;
@group(0) @binding(1) var<storage> source_mask: array<u32>;
@group(0) @binding(2) var<storage, read_write> destination_mask: array<atomic<u32>>;
@group(0) @binding(3) var<storage> tile_info: array<TileInfo>;

fn load_mask(tile_idx: u32, local_coord: vec2u) -> f32 {
    let linear = tile_idx * TILE_SIZE * TILE_SIZE + local_coord.y * TILE_SIZE + local_coord.x;
    let index = linear / 4u;
    let offset = (linear % 4u) * 8u;
    let value = (source_mask[index] >> offset) & 0xFFu;
    return f32(value) / 255.0;
}

fn store_mask(tile_idx: u32, local_coord: vec2u, value: f32) {
    let linear = tile_idx * TILE_SIZE * TILE_SIZE + local_coord.y * TILE_SIZE + local_coord.x;
    let index = linear / 4u;
    let offset = (linear % 4u) * 8u;
    let value_u32 = u32(value * 255.0) & 0xFFu;
    let byte_mask = 0xFFu << offset;
    atomicAnd(&destination_mask[index], ~byte_mask);
    atomicOr(&destination_mask[index], value_u32 << offset);
}


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
    for (var i = 0u; i < arrayLength(&tile_info); i = i + 1u) {
        let info = tile_info[i];
        if coord.x >= info.origin.x && coord.x < info.origin.x + i32(TILE_SIZE) &&
           coord.y >= info.origin.y && coord.y < info.origin.y + i32(TILE_SIZE) {
            return i32(i);
        }
    }
    return NO_TILE;
}

fn load_source_mask_global(coord: vec2i) -> f32 {
    if is_outside_image(coord) {
        return MASK_BARRIER;
    }

    let tile_index = find_source_tile_for_coord(coord);
    if tile_index == NO_TILE {
        return MASK_BARRIER;
    }

    let info = tile_info[u32(tile_index)];
    let local_coord = vec2u(coord - info.origin);
    return load_mask(u32(tile_index), local_coord);
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

fn dilate_pixel(global_coord: vec2i, radius: i32) -> bool {
    let radius_squared = radius * radius;

    for (var dy = -radius; dy <= radius; dy = dy + 1) {
        let x_limit = row_disk_x_limit(dy, radius, radius_squared);
        for (var dx = -x_limit; dx <= x_limit; dx = dx + 1) {
            if dx * dx + dy * dy <= radius_squared &&
               load_source_mask_global(global_coord + vec2i(dx, dy)) == MASK_FILLABLE {
                return true;
            }
        }
    }

    return false;
}

fn erode_pixel(global_coord: vec2i, radius: i32) -> bool {
    let radius_squared = radius * radius;

    for (var dy = -radius; dy <= radius; dy = dy + 1) {
        let x_limit = row_disk_x_limit(dy, radius, radius_squared);
        for (var dx = -x_limit; dx <= x_limit; dx = dx + 1) {
            if dx * dx + dy * dy <= radius_squared &&
               load_source_mask_global(global_coord + vec2i(dx, dy)) != MASK_FILLABLE {
                return false;
            }
        }
    }

    return true;
}

@compute
@workgroup_size(16, 16, 1)
fn grow(@builtin(global_invocation_id) global_id: vec3u) {
    let tile_index = global_id.z;
    let local_coord = global_id.xy;

    let tile = tile_info[tile_index];
    let global_coord = tile.origin + vec2i(local_coord);
    if is_outside_image(global_coord) {
        store_mask(tile_index, local_coord, MASK_BARRIER);
        return;
    }

    var value = 0.0;
    if params.grow > 0 {
        let should_dilate = dilate_pixel(global_coord, params.grow);
        value = select(MASK_BARRIER, MASK_FILLABLE, should_dilate);
    } else if params.grow < 0 {
        let should_erode = erode_pixel(global_coord, -params.grow);
        value = select(MASK_FILLABLE, MASK_BARRIER, should_erode);
    }

    store_mask(tile_index, local_coord, value);
}
