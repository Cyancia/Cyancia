import image::image_tiling::{TileInfo, TILE_SIZE};

@group(0) @binding(0) var input: texture_storage_2d_array<r8unorm, read>;
@group(0) @binding(1) var<storage> input_tile_info: array<TileInfo>;
@group(0) @binding(2) var output: texture_storage_2d_array<r8unorm, read_write>;
@group(0) @binding(3) var<storage> output_tile_info: array<TileInfo>;
@group(0) @binding(4) var<uniform> params: SelectionParams;

struct SelectionParams {
    // 0 - Replace
    // 1 - Intersect
    // 2 - Union
    // 3 - Subtract
    // 4 - Symmetric Difference
    operation_ty: u32,
}

fn get_input_selection(global_coord: vec2i) -> f32 {
    for (var i = 0u; i < arrayLength(&input_tile_info); i += 1u) {
        let tile = input_tile_info[i];
        if all(global_coord >= tile.origin) && all(global_coord < tile.origin + i32(TILE_SIZE)) {
            let local_coord = global_coord - tile.origin;
            let lhs = textureLoad(input, local_coord, i).r;
            return lhs;
        }
    }
    return 0.0;
}

@compute
@workgroup_size(16, 16, 1)
fn main(@builtin(global_invocation_id) global_id: vec3u) {
    let tile_index = global_id.z;
    let local_coord = global_id.xy;
    let global_coord = output_tile_info[tile_index].origin + vec2i(local_coord);

    let lhs = get_input_selection(global_coord);
    let rhs = textureLoad(output, local_coord, tile_index).r;

    var result = 0.0;
    switch params.operation_ty {
        case 0u: {
            result = rhs;
            break;
        }
        case 1u: {
            result = min(lhs, rhs);
            break;
        }
        case 2u: {
            result = max(lhs, rhs);
            break;
        }
        case 3u: {
            result = max(0.0, lhs - rhs);
            break;
        }
        case 4u: {
            result = abs(lhs - rhs);
            break;
        }
        default: {
            break;
        }
    }

    textureStore(output, local_coord, tile_index, vec4f(result, 0.0, 0.0, 0.0));
}
