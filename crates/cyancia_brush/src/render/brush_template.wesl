struct GraphInput {
    shader_origin: vec2i,
    estimated_brush_size: vec2u,
    pen_position: vec2f,
    tile_size: u32,
}

struct TileInfo {
    tile_origin: vec2i,
}

@group(0) @binding(0) var<storage> graph_input: GraphInput;
// TODO: This is weird. This is a bindless array, and theoretically it won't have a fixed size.
//       It doesn't compile if remove the size, but works if it's set to a random value. Why?
@group(0) @binding(1) var outputs: binding_array<texture_storage_2d<rgba16float, write>, 1>;
@group(0) @binding(2) var textures: binding_array<texture_2d<f32>>;
@group(0) @binding(3) var<storage> tile_info: array<TileInfo>;

fn convert_pixel_to_tile(pixel: vec2i, layer: ptr<function, u32>, coord: ptr<function, vec2u>) {
    for (var i = 0u; i < arrayLength(&tile_info); i = i + 1u) {
        let info = tile_info[i];
        if (pixel.x >= info.tile_origin.x && pixel.x < info.tile_origin.x + i32(graph_input.tile_size) &&
            pixel.y >= info.tile_origin.y && pixel.y < info.tile_origin.y + i32(graph_input.tile_size)) {
            *layer = i;
            *coord = vec2u(pixel - info.tile_origin);
            return;
        }
    }
}

@compute
@workgroup_size(16, 16, 1)
fn main(@builtin(global_invocation_id) id: vec3u) {
    if (id.x >= graph_input.estimated_brush_size.x || id.y >= graph_input.estimated_brush_size.y) {
        return;
    }
    let pixel_pos = vec2i(id.xy) + graph_input.shader_origin;
    let pixel_posf = vec2f(pixel_pos);

    //CODEGENFLAG_COMPILED_GRAPH
}
