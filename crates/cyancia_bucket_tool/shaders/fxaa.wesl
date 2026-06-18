import image::image_tiling::{TileInfo, TILE_SIZE};

const MASK_OFF = 0.0;
const MASK_ON = 1.0;
const MIN_MASK_VALUE = 0.0;
const MAX_MASK_VALUE = 1.0;
const NO_TILE = -1;
const HORIZONTAL_EDGE = vec2i(1, 0);
const VERTICAL_EDGE = vec2i(0, 1);
const DIAGONAL_UP_EDGE = vec2i(1, -1);
const DIAGONAL_DOWN_EDGE = vec2i(1, 1);

struct FxaaParams {
    edge_threshold_min: f32,
    edge_threshold_max: f32,
    iterations: u32,
    subpixel_quality: f32,
    image_size: vec2u,
};

@group(0) @binding(0) var input: texture_storage_2d_array<r8unorm, read>;
@group(0) @binding(1) var output: texture_storage_2d_array<r8unorm, write>;
@group(0) @binding(2) var<storage> tile_info: array<TileInfo>;
@group(0) @binding(3) var<uniform> params: FxaaParams;

fn find_tile_for_coord(coord: vec2i) -> i32 {
    for (var i = 0u; i < arrayLength(&tile_info); i = i + 1u) {
        let info = tile_info[i];
        if coord.x >= info.origin.x && coord.x < info.origin.x + i32(TILE_SIZE) &&
           coord.y >= info.origin.y && coord.y < info.origin.y + i32(TILE_SIZE) {
            return i32(i);
        }
    }
    return NO_TILE;
}

fn load_mask_global(coord: vec2i) -> f32 {
    let tile_index = find_tile_for_coord(coord);
    if tile_index == NO_TILE {
        return 0.0;
    }

    let info = tile_info[u32(tile_index)];
    let local_coord = vec2u(coord - info.origin);
    return textureLoad(input, local_coord, tile_index).r;
}

fn sample_mask_global_linear(coord: vec2f) -> f32 {
    let base = floor(coord);
    let base_coord = vec2i(base);
    let fract = coord - base;

    let left_top = load_mask_global(base_coord);
    let right_top = load_mask_global(base_coord + vec2i(1, 0));
    let left_bottom = load_mask_global(base_coord + vec2i(0, 1));
    let right_bottom = load_mask_global(base_coord + vec2i(1, 1));
    let top = mix(left_top, right_top, fract.x);
    let bottom = mix(left_bottom, right_bottom, fract.x);
    return mix(top, bottom, fract.y);
}

fn QUALITY(q: i32) -> f32 {
    switch q {
        //case 0, 1, 2, 3, 4: { return 1.0; }
        default: { return 1.0; }
        case 5: { return 1.5; }
        case 6, 7, 8, 9: { return 2.0; }
        case 10: { return 4.0; }
        case 11: { return 8.0; }
    }
}

// Copied and adopted from bevy
@compute
@workgroup_size(16, 16, 1)
fn main(@builtin(global_invocation_id) global_id: vec3u) {
    let tile_index = global_id.z;
    let local_coord = global_id.xy;
    let global_coord = vec2i(local_coord) + tile_info[tile_index].origin;

    let pixelCoord = vec2f(global_coord);

    let centerSample = sample_mask_global_linear(pixelCoord);
    let colorCenter = centerSample;

    // Luma at the current fragment
    let lumaCenter = colorCenter;

    // Luma at the four direct neighbors of the current fragment.
    let lumaDown = sample_mask_global_linear(pixelCoord + vec2f(0.0, -1.0));
    let lumaUp = sample_mask_global_linear(pixelCoord + vec2f(0.0, 1.0));
    let lumaLeft = sample_mask_global_linear(pixelCoord + vec2f(-1.0, 0.0));
    let lumaRight = sample_mask_global_linear(pixelCoord + vec2f(1.0, 0.0));

    // Find the maximum and minimum luma around the current fragment.
    let lumaMin = min(lumaCenter, min(min(lumaDown, lumaUp), min(lumaLeft, lumaRight)));
    let lumaMax = max(lumaCenter, max(max(lumaDown, lumaUp), max(lumaLeft, lumaRight)));

    // Compute the delta.
    let lumaRange = lumaMax - lumaMin;

    // If the luma variation is lower that a threshold (or if we are in a really dark area), we are not on an edge, don't perform any AA.
    if (lumaRange < max(params.edge_threshold_min, lumaMax * params.edge_threshold_max)) {
        textureStore(output, local_coord, tile_index, vec4f(centerSample, 0.0, 0.0, 1.0));
        return;
    }

    // Query the 4 remaining corners lumas.
    let lumaDownLeft  = sample_mask_global_linear(pixelCoord + vec2f(-1.0, -1.0));
    let lumaUpRight   = sample_mask_global_linear(pixelCoord + vec2f(1.0, 1.0));
    let lumaUpLeft    = sample_mask_global_linear(pixelCoord + vec2f(-1.0, 1.0));
    let lumaDownRight = sample_mask_global_linear(pixelCoord + vec2f(1.0, -1.0));

    // Combine the four edges lumas (using intermediary variables for future computations with the same values).
    let lumaDownUp = lumaDown + lumaUp;
    let lumaLeftRight = lumaLeft + lumaRight;

    // Same for corners
    let lumaLeftCorners = lumaDownLeft + lumaUpLeft;
    let lumaDownCorners = lumaDownLeft + lumaDownRight;
    let lumaRightCorners = lumaDownRight + lumaUpRight;
    let lumaUpCorners = lumaUpRight + lumaUpLeft;

    // Compute an estimation of the gradient along the horizontal and vertical axis.
    let edgeHorizontal = abs(-2.0 * lumaLeft   + lumaLeftCorners)  +
                         abs(-2.0 * lumaCenter + lumaDownUp) * 2.0 +
                         abs(-2.0 * lumaRight  + lumaRightCorners);

    let edgeVertical =   abs(-2.0 * lumaUp     + lumaUpCorners)       +
                         abs(-2.0 * lumaCenter + lumaLeftRight) * 2.0 +
                         abs(-2.0 * lumaDown   + lumaDownCorners);

    // Is the local edge horizontal or vertical ?
    let isHorizontal = (edgeHorizontal >= edgeVertical);

    // Choose the step size (one pixel) accordingly.
    var stepLength = 1.0;

    // Select the two neighboring texels lumas in the opposite direction to the local edge.
    var luma1 = select(lumaLeft, lumaDown, isHorizontal);
    var luma2 = select(lumaRight, lumaUp, isHorizontal);

    // Compute gradients in this direction.
    let gradient1 = luma1 - lumaCenter;
    let gradient2 = luma2 - lumaCenter;

    // Which direction is the steepest ?
    let is1Steepest = abs(gradient1) >= abs(gradient2);

    // Gradient in the corresponding direction, normalized.
    let gradientScaled = 0.25 * max(abs(gradient1), abs(gradient2));

    // Average luma in the correct direction.
    var lumaLocalAverage = 0.0;
    if (is1Steepest) {
        // Switch the direction
        stepLength = -stepLength;
        lumaLocalAverage = 0.5 * (luma1 + lumaCenter);
    } else {
        lumaLocalAverage = 0.5 * (luma2 + lumaCenter);
    }

    // Shift UV in the correct direction by half a pixel.
    // Compute offset (for each iteration step) in the right direction.
    var currentCoord = pixelCoord;
    var offset = vec2<f32>(0.0, 0.0);
    if (isHorizontal) {
        currentCoord.y = currentCoord.y + stepLength * 0.5;
        offset.x = 1.0;
    } else {
        currentCoord.x = currentCoord.x + stepLength * 0.5;
        offset.y = 1.0;
    }

    // Compute coordinates to explore on each side of the edge, orthogonally. The QUALITY allows us to step faster.
    var coord1 = currentCoord - offset; // * QUALITY(0); // (quality 0 is 1.0)
    var coord2 = currentCoord + offset; // * QUALITY(0); // (quality 0 is 1.0)

    // Read the lumas at both current extremities of the exploration segment, and compute the delta wrt to the local average luma.
    var lumaEnd1 = sample_mask_global_linear(coord1);
    var lumaEnd2 = sample_mask_global_linear(coord2);
    lumaEnd1 = lumaEnd1 - lumaLocalAverage;
    lumaEnd2 = lumaEnd2 - lumaLocalAverage;

    // If the luma deltas at the current extremities is larger than the local gradient, we have reached the side of the edge.
    var reached1 = abs(lumaEnd1) >= gradientScaled;
    var reached2 = abs(lumaEnd2) >= gradientScaled;
    var reachedBoth = reached1 && reached2;

    // If the side is not reached, we continue to explore in this direction.
    coord1 = select(coord1 - offset, coord1, reached1); // * QUALITY(1); // (quality 1 is 1.0)
    coord2 = select(coord2 + offset, coord2, reached2); // * QUALITY(1); // (quality 1 is 1.0)

    // If both sides have not been reached, continue to explore.
    if (!reachedBoth) {
        for (var i: i32 = 2; i < i32(params.iterations); i = i + 1) {
            // If needed, read luma in 1st direction, compute delta.
            if (!reached1) {
                lumaEnd1 = sample_mask_global_linear(coord1);
                lumaEnd1 = lumaEnd1 - lumaLocalAverage;
            }
            // If needed, read luma in opposite direction, compute delta.
            if (!reached2) {
                lumaEnd2 = sample_mask_global_linear(coord2);
                lumaEnd2 = lumaEnd2 - lumaLocalAverage;
            }
            // If the luma deltas at the current extremities is larger than the local gradient, we have reached the side of the edge.
            reached1 = abs(lumaEnd1) >= gradientScaled;
            reached2 = abs(lumaEnd2) >= gradientScaled;
            reachedBoth = reached1 && reached2;

            // If the side is not reached, we continue to explore in this direction, with a variable quality.
            if (!reached1) {
                coord1 = coord1 - offset * QUALITY(i);
            }
            if (!reached2) {
                coord2 = coord2 + offset * QUALITY(i);
            }

            // If both sides have been reached, stop the exploration.
            if (reachedBoth) {
                break;
            }
        }
    }

    // Compute the distances to each side edge of the edge (!).
    var distance1 = select(pixelCoord.y - coord1.y, pixelCoord.x - coord1.x, isHorizontal);
    var distance2 = select(coord2.y - pixelCoord.y, coord2.x - pixelCoord.x, isHorizontal);

    // In which direction is the side of the edge closer ?
    let isDirection1 = distance1 < distance2;
    let distanceFinal = min(distance1, distance2);

    // Thickness of the edge.
    let edgeThickness = (distance1 + distance2);

    // Is the luma at center smaller than the local average ?
    let isLumaCenterSmaller = lumaCenter < lumaLocalAverage;

    // If the luma at center is smaller than at its neighbor, the delta luma at each end should be positive (same variation).
    let correctVariation1 = (lumaEnd1 < 0.0) != isLumaCenterSmaller;
    let correctVariation2 = (lumaEnd2 < 0.0) != isLumaCenterSmaller;

    // Only keep the result in the direction of the closer side of the edge.
    var correctVariation = select(correctVariation2, correctVariation1, isDirection1);

    // UV offset: read in the direction of the closest side of the edge.
    let pixelOffset = - distanceFinal / edgeThickness + 0.5;

    // If the luma variation is incorrect, do not offset.
    var finalOffset = select(0.0, pixelOffset, correctVariation);

    // Sub-pixel shifting
    // Full weighted average of the luma over the 3x3 neighborhood.
    let lumaAverage = (1.0 / 12.0) * (2.0 * (lumaDownUp + lumaLeftRight) + lumaLeftCorners + lumaRightCorners);
    // Ratio of the delta between the global average and the center luma, over the luma range in the 3x3 neighborhood.
    let subPixelOffset1 = clamp(abs(lumaAverage - lumaCenter) / lumaRange, 0.0, 1.0);
    let subPixelOffset2 = (-2.0 * subPixelOffset1 + 3.0) * subPixelOffset1 * subPixelOffset1;
    // Compute a sub-pixel offset based on this delta.
    let subPixelOffsetFinal = subPixelOffset2 * subPixelOffset2 * params.subpixel_quality;

    // Pick the biggest of the two offsets.
    finalOffset = max(finalOffset, subPixelOffsetFinal);

    // Compute the final sample coordinates.
    var finalCoord = pixelCoord;
    if (isHorizontal) {
        finalCoord.y = finalCoord.y + finalOffset * stepLength;
    } else {
        finalCoord.x = finalCoord.x + finalOffset * stepLength;
    }

    // Read the color at the new coordinates, and use it.
    var finalColor = sample_mask_global_linear(finalCoord);
    textureStore(output, local_coord, tile_index, vec4f(finalColor, 0.0, 0.0, 1.0));
}
