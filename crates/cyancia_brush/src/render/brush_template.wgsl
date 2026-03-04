struct GraphInput {
    pen_position: vec2f,
}

struct TileInfo {
    tile_size: vec2u,
    tile_index: vec2u,
}

@group(0) @binding(0) var<uniform> graph_input: GraphInput;
@group(0) @binding(1) var output: texture_storage_2d<rgba16float, write>;
@group(0) @binding(2) var<uniform> tile_info: TileInfo;
@group(0) @binding(3) var textures: binding_array<texture_2d<f32>>;

@compute
@workgroup_size(16, 16, 1)
fn main(@builtin(global_invocation_id) id: vec3u) {
    if (id.x >= tile_info.tile_size.x || id.y >= tile_info.tile_size.y) {
        return;
    }

    //CODEGENFLAG_COMPILED_GRAPH
}
