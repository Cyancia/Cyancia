import image::image_tiling::{TileInfo, TILE_SIZE};
import package::types::{BucketParams, INVALID_LABEL};

@group(0) @binding(0) var<uniform> params: BucketParams;
@group(0) @binding(1) var<storage, read_write> mask: array<atomic<u32>>;
@group(0) @binding(2) var<storage, read_write> labels: array<atomic<u32>>;
@group(0) @binding(3) var<storage> tile_info: array<TileInfo>;

fn load_mask(tile_idx: u32, local_coord: vec2u) -> f32 {
    let linear = tile_idx * TILE_SIZE * TILE_SIZE + local_coord.y * TILE_SIZE + local_coord.x;
    let index = linear / 4u;
    let offset = (linear % 4u) * 8u;
    let value = (atomicLoad(&mask[index]) >> offset) & 0xFFu;
    return f32(value) / 255.0;
}

fn store_mask(tile_idx: u32, local_coord: vec2u, value: f32) {
    let linear = tile_idx * TILE_SIZE * TILE_SIZE + local_coord.y * TILE_SIZE + local_coord.x;
    let index = linear / 4u;
    let offset = (linear % 4u) * 8u;
    let value_u32 = u32(value * 255.0) & 0xFFu;
    let byte_mask = 0xFFu << offset;
    atomicAnd(&mask[index], ~byte_mask);
    atomicOr(&mask[index], value_u32 << offset);
}

fn get_label(tile_idx: u32, local_coord: vec2u) -> u32 {
    return atomicLoad(&labels[get_linear_index(tile_idx, local_coord)]);
}

fn get_linear_index(tile_idx: u32, local_coord: vec2u) -> u32 {
    return tile_idx * TILE_SIZE * TILE_SIZE + local_coord.y * TILE_SIZE + local_coord.x;
}

fn find_tile_for_coord(coord: vec2i) -> i32 {
    for (var i = 0u; i < arrayLength(&tile_info); i++) {
        let info = tile_info[i];
        if coord.x >= info.origin.x && coord.x < info.origin.x + i32(TILE_SIZE) &&
           coord.y >= info.origin.y && coord.y < info.origin.y + i32(TILE_SIZE) {
            return i32(i);
        }
    }
    return -1;
}

fn find_root(idx: u32) -> u32 {
    var root = atomicLoad(&labels[idx]);
    while root != idx {
        let parent = atomicLoad(&labels[root]);
        if parent == root {
            break;
        }
        root = parent;
    }
    return root;
}

fn union_labels(idx_a: u32, idx_b: u32) {
    var root_a = find_root(idx_a);
    var root_b = find_root(idx_b);

    while root_a != root_b {
        if root_a < root_b {
            let old = atomicMin(&labels[root_b], root_a);
            if old == root_b {
                break;
            }
            root_b = find_root(root_b);
        } else {
            let old = atomicMin(&labels[root_a], root_b);
            if old == root_a {
                break;
            }
            root_a = find_root(root_a);
        }
    }
}

@compute @workgroup_size(16, 16, 1)
fn ccl_init(@builtin(global_invocation_id) gid: vec3u) {
    let tile_idx = gid.z;
    let pixel_idx = gid.xy;

    let linear_idx = tile_idx * TILE_SIZE * TILE_SIZE + pixel_idx.y * TILE_SIZE + pixel_idx.x;
    let mask_value = load_mask(tile_idx, pixel_idx);
    labels[linear_idx] = select(INVALID_LABEL, linear_idx, mask_value == 1.0);
}

@compute @workgroup_size(16, 16, 1)
fn ccl_merge(@builtin(global_invocation_id) gid: vec3u) {
    let tile_idx = gid.z;
    if tile_idx >= arrayLength(&tile_info) {
        return;
    }

    let info = tile_info[tile_idx];
    let local_coord = gid.xy;
    if any(local_coord >= vec2u(TILE_SIZE)) {
        return;
    }

    let idx = get_label(tile_idx, local_coord);
    if idx == INVALID_LABEL {
        return;
    }

    atomicStore(&labels[idx], idx);

    let global_coord = info.origin + vec2i(local_coord);
    let offsets = array<vec2i, 2>(vec2i(-1, 0), vec2i(0, -1));

    for (var i = 0; i < 2; i++) {
        let neighbor_global = global_coord + offsets[i];
        let neighbor_tile_idx = find_tile_for_coord(neighbor_global);

        if neighbor_tile_idx < 0 {
            continue;
        }

        let neighbor_info = tile_info[neighbor_tile_idx];
        let neighbor_local = vec2u(neighbor_global - neighbor_info.origin);

        let neighbor_idx = get_label(u32(neighbor_tile_idx), neighbor_local);
        if neighbor_idx != INVALID_LABEL {
            union_labels(idx, neighbor_idx);
        }
    }
}

@compute @workgroup_size(16, 16, 1)
fn ccl_compress(@builtin(global_invocation_id) gid: vec3u) {
    let tile_idx = gid.z;
    if tile_idx >= arrayLength(&tile_info) {
        return;
    }

    let local_coord = gid.xy;
    if any(local_coord >= vec2u(TILE_SIZE)) {
        return;
    }

    let idx = get_label(tile_idx, local_coord);
    if idx != INVALID_LABEL {
        let root = find_root(idx);
        atomicStore(&labels[idx], root);
    }
}

@compute @workgroup_size(16, 16, 1)
fn ccl_extract(@builtin(global_invocation_id) gid: vec3u) {
    let tile_idx = gid.z;
    if tile_idx >= arrayLength(&tile_info) {
        return;
    }

    let local_coord = gid.xy;
    if any(local_coord >= vec2u(TILE_SIZE)) {
        return;
    }

    let seed_tile = find_tile_for_coord(vec2i(params.seed));
    if seed_tile < 0 {
        return;
    }

    let seed_info = tile_info[seed_tile];
    let seed_local = vec2u(vec2i(params.seed) - seed_info.origin);
    let seed_idx = get_label(u32(seed_tile), seed_local);
    if seed_idx == INVALID_LABEL {
        return;
    }

    let seed_root = atomicLoad(&labels[seed_idx]);

    let idx = get_label(tile_idx, local_coord);
    let value = select(0.0, 1.0, idx != INVALID_LABEL && atomicLoad(&labels[idx]) == seed_root);
    store_mask(tile_idx, local_coord, value);
}
