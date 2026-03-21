import image::image_tiling::{TileInfo, TILE_SIZE};

struct StrokeInfo {
    bound_min: vec2i,
    bound_max: vec2i,
    accumulated_bound_min: vec2i,
    accumulated_bound_max: vec2i,
}

struct DynamicTileInfo {
    n_tiles: atomic<u32>,
    buf: array<TileInfo>,
}

@group(0) @binding(0) var<storage, read> stroke_info: StrokeInfo;
@group(0) @binding(1) var<storage, read_write> tile_info: DynamicTileInfo;

@compute
@workgroup_size(8, 8, 1)
fn allocate(@builtin(global_invocation_id) global_id: vec3u) {
    let tile_index = stroke_info.bound_min + vec2i(global_id.xy);
    if any(tile_index >= stroke_info.bound_max) {
        return;
    }

    var allocated = false;
    for (var i = 0u; i < tile_info.n_tiles; i++) {
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
