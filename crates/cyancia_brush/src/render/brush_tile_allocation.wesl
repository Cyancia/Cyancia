import package::image::image_tiling::{TileInfo, TILE_SIZE};
import package::brush::brush_types::{DabInfos, DynamicTileInfo, StrokeInfo};

@group(0) @binding(0) var<storage> dab_infos: DabInfos;
@group(0) @binding(1) var<storage, read_write> tile_info: DynamicTileInfo;
@group(0) @binding(2) var<storage> stroke_info: StrokeInfo;

@if(!POSTPROCESS)
@compute
@workgroup_size(1, 1, 8)
fn allocate(@builtin(global_invocation_id) id: vec3u) {
    let sample_index = id.z;
    if sample_index > dab_infos.n_dabs {
        return;
    }
    let dab_info = dab_infos.buf[sample_index];

    let tile_index = dab_info.bound_min + vec2i(id.xy);
    if any(tile_index >= dab_info.bound_max) {
        return;
    }

    // We need to ensure that only one invocation handles allocation at certain index.
    // Here we check if there's another dab after this one also allocates this tile.
    // If there is, skip this invocation.
    // If there isn't, then this invocation will be the only one allocating this tile.
    for (var i = sample_index + 1u; i < dab_infos.n_dabs; i++) {
        if all(dab_infos.buf[i].bound_min <= tile_index) && all(dab_infos.buf[i].bound_max > tile_index) {
            return;
        }
    }

    var allocated = false;
    let n_tiles = atomicLoad(&tile_info.n_tiles);
    for (var i = 0u; i < n_tiles; i++) {
        if all(tile_info.buf[i].index == tile_index) {
            allocated = true;
            break;
        }
    }
    if allocated {
        return;
    }

    let new_index = atomicAdd(&tile_info.n_tiles, 1u);
    let buffer_max = arrayLength(&tile_info.buf);
    if new_index < buffer_max {
        tile_info.buf[new_index] = TileInfo(tile_index, tile_index * i32(TILE_SIZE));
    } else {
        // TODO prevent overflow
        atomicMax(&tile_info.n_tiles, buffer_max);
    }
}

@if(POSTPROCESS)
@compute
@workgroup_size(1, 1, 1)
fn allocate(@builtin(global_invocation_id) id: vec3u) {
    let tile_index = stroke_info.accumulated_bound_min + vec2i(id.xy);
    if any(tile_index >= stroke_info.accumulated_bound_max) {
        return;
    }

    var allocated = false;
    let n_tiles = atomicLoad(&tile_info.n_tiles);
    for (var i = 0u; i < n_tiles; i++) {
        if all(tile_info.buf[i].index == tile_index) {
            allocated = true;
            break;
        }
    }
    if allocated {
        return;
    }

    let new_index = atomicAdd(&tile_info.n_tiles, 1u);
    let buffer_max = arrayLength(&tile_info.buf);
    if new_index < buffer_max {
        tile_info.buf[new_index] = TileInfo(tile_index, tile_index * i32(TILE_SIZE));
    } else {
        // TODO prevent overflow
        atomicMax(&tile_info.n_tiles, buffer_max);
    }
}
