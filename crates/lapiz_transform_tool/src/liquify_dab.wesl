import image::image_tiling::{TileInfo, TILE_SIZE};

const MODE_MOVE: u32 = 0u;
const MODE_SCALE: u32 = 1u;
const MODE_ROTATE: u32 = 2u;
const MODE_OFFSET: u32 = 3u;
const MODE_UNDO: u32 = 4u;

struct DabParams {
    center: vec2f,
    dir: vec2f,
    mode: u32,
    sigma: f32,
    magnitude: f32,
};

@group(0) @binding(0) var disp_src: texture_storage_2d_array<r32uint, read>;
@group(0) @binding(1) var disp_dst: texture_storage_2d_array<r32uint, write>;
@group(0) @binding(2) var<storage> disp_src_tile_info: array<TileInfo>;
@group(0) @binding(3) var<storage> disp_dst_tile_info: array<TileInfo>;
@group(0) @binding(4) var<uniform> dab_params: DabParams;

fn load_disp_texel(pos: vec2i) -> vec2f {
    for (var i = 0u; i < arrayLength(&disp_src_tile_info); i += 1u) {
        let tile = disp_src_tile_info[i];
        if all(pos >= tile.origin) && all(pos < tile.origin + i32(TILE_SIZE)) {
            return unpack2x16float(textureLoad(disp_src, vec2u(pos - tile.origin), i).r);
        }
    }
    return vec2f(0.0);
}

fn sample_disp(pos: vec2f) -> vec2f {
    let p0 = floor(pos);
    let t = pos - p0;
    let i0 = vec2i(p0);
    let f00 = load_disp_texel(i0);
    let f10 = load_disp_texel(i0 + vec2i(1, 0));
    let f01 = load_disp_texel(i0 + vec2i(0, 1));
    let f11 = load_disp_texel(i0 + vec2i(1, 1));
    return mix(mix(f00, f10, t.x), mix(f01, f11, t.x), t.y);
}

fn dab_displacement(diff: vec2f, lambda: f32) -> vec2f {
    if dab_params.mode == MODE_MOVE {
        return -dab_params.dir * lambda;
    }
    if dab_params.mode == MODE_SCALE {
        return -dab_params.magnitude * lambda * diff;
    }
    if dab_params.mode == MODE_ROTATE {
        return dab_params.magnitude * lambda * vec2f(-diff.y, diff.x);
    }
    if dab_params.mode == MODE_OFFSET {
        return dab_params.magnitude * lambda * vec2f(dab_params.dir.y, -dab_params.dir.x);
    }
    if dab_params.mode == MODE_UNDO {
        return vec2f(0.0);
    }
    return vec2f(0.0);
}

@compute
@workgroup_size(16, 16, 1)
fn dab(@builtin(global_invocation_id) global_id: vec3u) {
    let tile_index = global_id.z;
    let local = global_id.xy;
    let pos = disp_dst_tile_info[tile_index].origin + vec2i(local);

    let diff = vec2f(pos) - dab_params.center;
    let sigma_sq = dab_params.sigma * dab_params.sigma;
    let lambda = exp(-0.5 * dot(diff, diff) / sigma_sq);

    var f = vec2f(0.0);
    if dab_params.mode == MODE_UNDO {
        f = sample_disp(vec2f(pos)) * (1.0 - dab_params.magnitude * lambda);
    } else {
        let d = dab_displacement(diff, lambda);
        f = sample_disp(vec2f(pos) + d) + d;
    }

    f = clamp(f, vec2f(-2048.0), vec2f(2048.0));
    textureStore(disp_dst, local, tile_index, vec4u(pack2x16float(f), 0u, 0u, 0u));
}
