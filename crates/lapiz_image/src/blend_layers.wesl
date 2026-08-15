import image::image_tiling::{TileInfo, TILE_SIZE};
import image::texture_unpack::{pack_rgba8_texel, unpack_rgba8_texel};

struct BlendLayerParams {
    src_opacity: f32,
    src_disabled_channels: u32,
}

@group(0) @binding(0) var<uniform> params: BlendLayerParams;
@group(0) @binding(1) var src: texture_storage_2d_array<r32uint, read>;
@group(0) @binding(2) var<storage> src_tile_info: array<TileInfo>;
@group(0) @binding(3) var dst: texture_storage_2d_array<r32uint, read>;
@group(0) @binding(4) var<storage> dst_tile_info: array<TileInfo>;
@group(0) @binding(5) var output: texture_storage_2d_array<r32uint, write>;
@group(0) @binding(6) var<storage> output_tile_info: array<TileInfo>;
@if(OVERRIDER) @group(0) @binding(7) var src_overrider: texture_storage_2d_array<r32uint, read>;
@if(OVERRIDER) @group(0) @binding(8) var<storage> src_overrider_tile_info: array<TileInfo>;

fn convert_pixel_to_src_tile(pixel: vec2i) -> vec3u {
    for (var i = 0u; i < arrayLength(&src_tile_info); i = i + 1u) {
        let info = src_tile_info[i];
        if pixel.x >= info.origin.x && pixel.x < info.origin.x + i32(TILE_SIZE) &&
            pixel.y >= info.origin.y && pixel.y < info.origin.y + i32(TILE_SIZE) {
            return vec3u(vec2u(pixel - info.origin), i);
        }
    }
    return vec3u(0, 0, 0xffffffffu);
}

fn convert_pixel_to_dst_tile(pixel: vec2i) -> vec3u {
    for (var i = 0u; i < arrayLength(&dst_tile_info); i = i + 1u) {
        let info = dst_tile_info[i];
        if pixel.x >= info.origin.x && pixel.x < info.origin.x + i32(TILE_SIZE) &&
            pixel.y >= info.origin.y && pixel.y < info.origin.y + i32(TILE_SIZE) {
            return vec3u(vec2u(pixel - info.origin), i);
        }
    }
    return vec3u(0, 0, 0xffffffffu);
}

fn convert_pixel_to_output_tile(pixel: vec2i) -> vec3u {
    for (var i = 0u; i < arrayLength(&output_tile_info); i = i + 1u) {
        let info = output_tile_info[i];
        if pixel.x >= info.origin.x && pixel.x < info.origin.x + i32(TILE_SIZE) &&
            pixel.y >= info.origin.y && pixel.y < info.origin.y + i32(TILE_SIZE) {
            return vec3u(vec2u(pixel - info.origin), i);
        }
    }
    return vec3u(0, 0, 0xffffffffu);
}

@if(OVERRIDER)
fn convert_pixel_to_src_overrider_tile(pixel: vec2i) -> vec3u {
    for (var i = 0u; i < arrayLength(&src_overrider_tile_info); i = i + 1u) {
        let info = src_overrider_tile_info[i];
        if pixel.x >= info.origin.x && pixel.x < info.origin.x + i32(TILE_SIZE) &&
            pixel.y >= info.origin.y && pixel.y < info.origin.y + i32(TILE_SIZE) {
            return vec3u(vec2u(pixel - info.origin), i);
        }
    }
    return vec3u(0, 0, 0xffffffffu);
}

fn get_src_color(pixel: vec2i) -> vec4f {
    @if(OVERRIDER) {
        let overrider_tile = convert_pixel_to_src_overrider_tile(pixel);
        if overrider_tile.z != 0xffffffffu {
            return unpack_rgba8_texel(textureLoad(src_overrider, overrider_tile.xy, overrider_tile.z));
        }
    }

    let tile = convert_pixel_to_src_tile(pixel);
    if tile.z == 0xffffffffu {
        return vec4f(0.0);
    }
    return unpack_rgba8_texel(textureLoad(src, tile.xy, tile.z));
}

fn get_dst_color(pixel: vec2i) -> vec4f {
    let tile = convert_pixel_to_dst_tile(pixel);
    if tile.z == 0xffffffffu {
        return vec4f(0.0);
    }
    return unpack_rgba8_texel(textureLoad(dst, tile.xy, tile.z));
}

fn set_output_color(pixel: vec2i, color: vec4f) {
    let tile = convert_pixel_to_output_tile(pixel);
    if tile.z == 0xffffffffu {
        return;
    }
    textureStore(output, tile.xy, tile.z, pack_rgba8_texel(color));
}

fn blend(src: vec4f, dst: vec4f) -> vec4f {
    //CODEGEN_BLEND_FUNC
}

fn is_channel_disabled(channel: u32) -> bool {
    return (params.src_disabled_channels & (1u << channel)) != 0u;
}

@compute
@workgroup_size(16, 16, 1)
fn main(@builtin(global_invocation_id) id: vec3u) {
    let pixel = id.xy;
    let src_color = get_src_color(vec2i(pixel));
    let dst_color = get_dst_color(vec2i(pixel));
    let blended_color = blend(vec4f(src_color.rgb, src_color.a * params.src_opacity), dst_color);
    let disabled_channel_factor = vec4f(
        select(0.0, 1.0, is_channel_disabled(0)),
        select(0.0, 1.0, is_channel_disabled(1)),
        select(0.0, 1.0, is_channel_disabled(2)),
        select(0.0, 1.0, is_channel_disabled(3))
    );
    set_output_color(vec2i(pixel), mix(blended_color, dst_color, disabled_channel_factor));
}
