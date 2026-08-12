import image::image_tiling::{TileInfo, TILE_SIZE};

struct AtomicBounds {
    min_x: atomic<i32>,
    min_y: atomic<i32>,
    max_x: atomic<i32>,
    max_y: atomic<i32>,
}

@if(RGBA8)
@group(0) @binding(0) var layer : texture_storage_2d_array<r32uint, read>;
@if(A8)
@group(0) @binding(0) var layer : texture_storage_2d_array<r8unorm, read>;

@group(0) @binding(1) var<storage> tile_info: array<TileInfo>;
@group(0) @binding(2) var<storage, read_write> tile_bounds: AtomicBounds;

@if(WITH_SELECTION)
@group(0) @binding(3) var<storage> has_selection: u32;
@if(WITH_SELECTION)
@group(0) @binding(4) var selection: texture_storage_2d_array<r8unorm, read>;
@if(WITH_SELECTION)
@group(0) @binding(5) var<storage> selection_tile_info: array<TileInfo>;

@if(WITH_SELECTION)
fn sample_selection(position_ps: vec2i) -> f32 {
    if has_selection == 0u {
        return 1.0;
    }

    for (var i = 0u; i < arrayLength(&selection_tile_info); i += 1u) {
        let tile = selection_tile_info[i];
        if all(position_ps >= tile.origin) && all(position_ps < tile.origin + i32(TILE_SIZE)) {
            return textureLoad(selection, position_ps - tile.origin, i).r;
        }
    }
    return 0.0;
}

@compute
@workgroup_size(16, 16, 1)
fn main(@builtin(global_invocation_id) global_id: vec3u) {
    let tile_index = global_id.z;
    let local_index = global_id.xy;
    let position_ps = tile_info[tile_index].origin + vec2i(local_index);

    var is_opaque = false;
    @if(RGBA8) {
        let unpacked = image::texture_unpack::unpack_rgba8_texel(textureLoad(layer, local_index, tile_index));
        is_opaque = unpacked.a > 0.0;
    }
    @if(A8) {
        is_opaque = textureLoad(layer, local_index, tile_index).r > 0.0;
    }
    @if(WITH_SELECTION) {
        if has_selection != 0u {
            is_opaque = sample_selection(position_ps) > 0.0;
        }
    }

    if is_opaque {
        atomicMin(&tile_bounds.min_x, position_ps.x);
        atomicMin(&tile_bounds.min_y, position_ps.y);
        atomicMax(&tile_bounds.max_x, position_ps.x + 1);
        atomicMax(&tile_bounds.max_y, position_ps.y + 1);
    }
}
