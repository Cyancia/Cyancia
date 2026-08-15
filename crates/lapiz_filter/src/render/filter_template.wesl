import image::image_tiling::{TileInfo, TILE_SIZE};
import render::math::{Rect, IRect, URect};

@group(0) @binding(0) var input_atlas: texture_storage_2d_array<r32uint, read>;
@group(0) @binding(1) var<storage> input_tile_info: array<TileInfo>;
@if(!BOUNDS_EVAL) @group(0) @binding(2) var output_atlas: texture_storage_2d_array<r32uint, write>;
@if(!BOUNDS_EVAL) @group(0) @binding(3) var<storage> output_tile_info: array<TileInfo>;
@group(0) @binding(4) var selection: texture_storage_2d_array<r8unorm, read>;
@group(0) @binding(5) var<storage> selection_tile_info: array<TileInfo>;
@group(0) @binding(6) var<storage> has_selection: u32;
@group(0) @binding(7) var<storage> bounds_input: IRect;
@if(BOUNDS_EVAL) @group(0) @binding(8) var<storage, read_write> bounds_output: IRect;
@if(!BOUNDS_EVAL) @group(0) @binding(9) var layer_atlas: texture_storage_2d_array<r32uint, read>;
@if(!BOUNDS_EVAL) @group(0) @binding(10) var<storage> layer_tile_info: array<TileInfo>;
@group(0) @binding(11) var textures: texture_2d<f32>;
@group(0) @binding(12) var<storage> texture_bounds: array<URect>;

//CODEGENFLAG_EXTERNAL_VARIABLE_BINDINGS

fn convert_pixel_to_input_tile(pixel: vec2i) -> vec3u {
    for (var i = 0u; i < arrayLength(&input_tile_info); i = i + 1u) {
        let info = input_tile_info[i];
        if pixel.x >= info.origin.x && pixel.x < info.origin.x + i32(TILE_SIZE) &&
            pixel.y >= info.origin.y && pixel.y < info.origin.y + i32(TILE_SIZE) {
            return vec3u(vec2u(pixel - info.origin), i);
        }
    }
    return vec3u(0, 0, 0xFFFFFFFFu);
}

@if(!BOUNDS_EVAL)
fn convert_pixel_to_output_tile(pixel: vec2i) -> vec3u {
    for (var i = 0u; i < arrayLength(&output_tile_info); i = i + 1u) {
        let info = output_tile_info[i];
        if pixel.x >= info.origin.x && pixel.x < info.origin.x + i32(TILE_SIZE) &&
            pixel.y >= info.origin.y && pixel.y < info.origin.y + i32(TILE_SIZE) {
            return vec3u(vec2u(pixel - info.origin), i);
        }
    }
    return vec3u(0, 0, 0xFFFFFFFFu);
}

@if(!BOUNDS_EVAL)
fn convert_pixel_to_layer_tile(pixel: vec2i) -> vec3u {
    for (var i = 0u; i < arrayLength(&layer_tile_info); i = i + 1u) {
        let info = layer_tile_info[i];
        if pixel.x >= info.origin.x && pixel.x < info.origin.x + i32(TILE_SIZE) &&
            pixel.y >= info.origin.y && pixel.y < info.origin.y + i32(TILE_SIZE) {
            return vec3u(vec2u(pixel - info.origin), i);
        }
    }
    return vec3u(0, 0, 0xFFFFFFFFu);
}

fn sample_local_texture(local_index: u32, pixel: vec2u) -> vec4f {
    let bounds = texture_bounds[local_index];
    if any(pixel >= bounds.max - bounds.min) {
        return vec4f(0.0);
    }
    return textureLoad(textures, pixel + bounds.min, 0);
}

// Load the input color at an arbitrary pixel.  Out-of-bounds / missing tiles return transparent.
fn load_input_color(pixel: vec2i) -> vec4f {
    @if(BOUNDS_EVAL) {
        // Bounds evaluation never samples actual pixel data.
        return vec4f(0.0);
    }
    @if(!BOUNDS_EVAL) {
        let tile_index = convert_pixel_to_input_tile(pixel);
        if tile_index.z == 0xFFFFFFFFu {
            return vec4f(0.0);
        }
        let color = textureLoad(input_atlas, vec2i(tile_index.xy), tile_index.z);
        return image::texture_unpack::unpack_rgba8_texel(color);
    }
}

// Brush-style helper names so the filter-specific graph nodes can mirror the
// brush graph nodes one-to-one.
fn current_input_color(pixel_pos: vec2i) -> vec4f {
    return load_input_color(pixel_pos);
}

// Color of the original layer (binding 9/10). Bounds evaluation never samples
// actual pixel data, so it returns transparent in that variant.
fn target_layer_color(pixel_pos: vec2i) -> vec4f {
    @if(BOUNDS_EVAL) {
        return vec4f(0.0);
    }
    @if(!BOUNDS_EVAL) {
        let tile_index = convert_pixel_to_layer_tile(pixel_pos);
        if tile_index.z == 0xFFFFFFFFu {
            return vec4f(0.0);
        }
        return image::texture_unpack::unpack_rgba8_texel(textureLoad(layer_atlas, vec2i(tile_index.xy), tile_index.z));
    }
}

fn load_selection_value(pixel: vec2i) -> f32 {
    if has_selection == 0u {
        return 1.0;
    }

    for (var i = 0u; i < arrayLength(&selection_tile_info); i += 1u) {
        let tile = selection_tile_info[i];
        if any(pixel < tile.origin) || any(pixel >= tile.origin + i32(TILE_SIZE)) {
            continue;
        }
        return textureLoad(selection, pixel - tile.origin, i).r;
    }

    return 0.0;
}

fn load_selection_mask_value(pixel_pos: vec2i) -> f32 {
    return load_selection_value(pixel_pos);
}

fn load_input_bounds() -> Rect {
    return Rect(vec2f(bounds_input.min), vec2f(bounds_input.max));
}

// Always defined so that an OutputColorNode in the graph compiles in both variants.
// The body is a no-op during bounds evaluation.
fn set_output_color(pixel_pos: vec2i, color: vec4f) {
    @if(!BOUNDS_EVAL) {
        let tile_index = convert_pixel_to_output_tile(pixel_pos);
        if tile_index.z == 0xFFFFFFFFu {
            return;
        }
        let packed = image::texture_unpack::pack_rgba8_texel(color);
        textureStore(output_atlas, vec2i(tile_index.xy), tile_index.z, packed);
    }
}

// Always defined so that an OutputBoundsNode in the graph compiles in both variants.
// The body only does real work during bounds evaluation.
fn set_output_bounds(bounds: Rect) {
    @if(BOUNDS_EVAL) {
        bounds_output.min = vec2i(bounds.min);
        bounds_output.max = vec2i(bounds.max);
    }
}

fn filter_graph(pixel: vec2i, pixel_posf: vec2f, input_bounds: Rect) {
    let pixel_pos = pixel;
    //CODEGENFLAG_COMPILED_GRAPH
}

@if(!BOUNDS_EVAL)
@compute
@workgroup_size(16, 16, 1)
fn filter_main(@builtin(global_invocation_id) global_id: vec3u) {
    let output_tile_index = global_id.z;
    if output_tile_index >= arrayLength(&output_tile_info) {
        return;
    }
    let input_bounds = load_input_bounds();
    let pixel = output_tile_info[output_tile_index].origin + vec2i(global_id.xy);
    filter_graph(pixel, vec2f(pixel), input_bounds);
}

@if(BOUNDS_EVAL)
@compute
@workgroup_size(1, 1, 1)
fn filter_bounds_eval(@builtin(global_invocation_id) global_id: vec3u) {
    let input_bounds = load_input_bounds();
    filter_graph(vec2i(0), vec2f(0), input_bounds);
}
