struct Rect {
    min: vec2f,
    max: vec2f,
}

struct IRect {
    min: vec2i,
    max: vec2i,
}

struct URect {
    min: vec2u,
    max: vec2u,
}

const PI: f32 = 3.14159265359;
const FRAC_PI_2: f32 = 1.57079632679;
const INV_PI: f32 = 0.31830988618;
const INV_FRAC_PI_2: f32 = 0.63661977236;
const TAU: f32 = 6.28318530718;
const INV_TAU: f32 = 0.15915494309;

fn div_ceil(lhs: u32, rhs: u32) -> u32 {
    let d = lhs / rhs;
    let r = lhs % rhs;
    if r > 0u {
        return d + 1u;
    } else {
        return d;
    }
}

fn inverse_mat3x3(matrix: mat3x3<f32>) -> mat3x3<f32> {
    let tmp0 = cross(matrix[1], matrix[2]);
    let tmp1 = cross(matrix[2], matrix[0]);
    let tmp2 = cross(matrix[0], matrix[1]);
    let inv_det = 1.0 / dot(matrix[2], tmp2);
    return transpose(mat3x3<f32>(tmp0 * inv_det, tmp1 * inv_det, tmp2 * inv_det));
}

fn transform_rect(rect: Rect, mat: mat3x3f) -> Rect {
    let p0 = mat * vec3f(rect.min, 1.0);
    let p1 = mat * vec3f(rect.max.x, rect.min.y, 1.0);
    let p2 = mat * vec3f(rect.min.x, rect.max.y, 1.0);
    let p3 = mat * vec3f(rect.max, 1.0);
    let min = min(min(p0.xy, p1.xy), min(p2.xy, p3.xy));
    let max = max(max(p0.xy, p1.xy), max(p2.xy, p3.xy));
    return Rect(min, max);
}

fn scale_rotate_transform(scale: vec2f, angle: f32, translation: vec2f) -> mat3x3f {
    let c = cos(angle);
    let s = sin(angle);
    return mat3x3f(
        vec3f(c * scale.x, s * scale.x, 0.0),
        vec3f(-s * scale.y, c * scale.y, 0.0),
        vec3f(translation, 1.0)
    );
}

fn scale_rotate_transform_inv(scale: vec2f, angle: f32, translation: vec2f) -> mat3x3f {
    let c = cos(angle);
    let s = sin(angle);
    let inv_scale = 1.0 / scale;
    let a_inv = mat2x2f(c * inv_scale.x, -s * inv_scale.y, s * inv_scale.x, c * inv_scale.y);
    return mat3x3f(
        vec3f(c * inv_scale.x, -s * inv_scale.y, 0.0),
        vec3f(s * inv_scale.x,  c * inv_scale.y, 0.0),
        vec3f(-(a_inv * translation), 1.0)
    );
}

fn scale_rotate_transform_anchored(scale: vec2f, angle: f32, translation: vec2f, anchor: vec2f) -> mat3x3f {
    let c = cos(angle);
    let s = sin(angle);
    let a = mat2x2f(c * scale.x, s * scale.x, -s * scale.y, c * scale.y);
    return mat3x3f(
        vec3f(c * scale.x, s * scale.x, 0.0),
        vec3f(-s * scale.y, c * scale.y, 0.0),
        vec3f(translation + anchor - (a * anchor), 1.0)
    );
}

fn scale_rotate_transform_anchored_inv(scale: vec2f, angle: f32, translation: vec2f, anchor: vec2f) -> mat3x3f {
    let c = cos(angle);
    let s = sin(angle);
    let inv_scale = 1.0 / scale;
    let a_inv = mat2x2f(c * inv_scale.x, -s * inv_scale.y, s * inv_scale.x, c * inv_scale.y);
    return mat3x3f(
        vec3f(c * inv_scale.x, -s * inv_scale.y, 0.0),
        vec3f(s * inv_scale.x,  c * inv_scale.y, 0.0),
        vec3f(anchor - a_inv * (translation + anchor), 1.0)
    );
}

const CUBIC_CURVE_MAX_CONTROL_POINTS: u32 = 16;

struct CubicCurve {
    control_points: array<vec2f, CUBIC_CURVE_MAX_CONTROL_POINTS>,
    derivatives: array<f32, CUBIC_CURVE_MAX_CONTROL_POINTS + 1>,
    num_control_points: u32,
}

fn sample_cubic_curve(curve: CubicCurve, x: f32) -> f32 {
    let n = curve.num_control_points;
    let cx = clamp(x, curve.control_points[0].x, curve.control_points[n - 1u].x);

    var i = 1u;
    while i < n - 1u && curve.control_points[i].x < cx {
        i += 1u;
    }

    let dx = curve.control_points[i].x - curve.control_points[i - 1u].x;
    let t = (cx - curve.control_points[i - 1u].x) / dx;
    let dy = curve.control_points[i].y - curve.control_points[i - 1u].y;

    let a = curve.derivatives[i - 1u] * dx - dy;
    let b = -curve.derivatives[i] * dx + dy;

    return saturate((1.0 - t) * curve.control_points[i - 1u].y + t * curve.control_points[i].y + t * (1.0 - t) * (a * (1.0 - t) + b * t));
}

fn unsigned_div_ceil(lhs: u32, rhs: u32) -> u32 {
    return (lhs + rhs - 1u) / rhs;
}

fn unsigned_div_ceil2(lhs: vec2u, rhs: vec2u) -> vec2u {
    return (lhs + rhs - 1u) / rhs;
}

// The MIT License
// Copyright © 2015 Inigo Quilez
// Permission is hereby granted, free of charge, to any person obtaining a copy of this software and associated documentation files (the "Software"), to deal in the Software without restriction, including without limitation the rights to use, copy, modify, merge, publish, distribute, sublicense, and/or sell copies of the Software, and to permit persons to whom the Software is furnished to do so, subject to the following conditions: The above copyright notice and this permission notice shall be included in all copies or substantial portions of the Software. THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.
// https://www.shadertoy.com/view/4lsXDN
fn sdf_ellipse(pixel: vec2f, center: vec2f, ab: vec2f) -> f32
{
    // symmetry
	let p = abs(pixel - center);

    // find root with Newton solver
    let q = ab * (p - ab);
    var w = 0.0;
    if q.x < q.y {
        w = 1.570796327;
    }

    for(var i = 0; i < 5; i++)
    {
        let cs = vec2f(cos(w), sin(w));
        let u = ab * vec2f(cs.x, cs.y);
        let v = ab * vec2f(-cs.y, cs.x);
        w = w + dot(p - u, v) / (dot(p - u, u) + dot(v, v));
    }

    // compute final point and distance
    let d = length(p - ab * vec2f(cos(w), sin(w)));

    // return signed distance
    if dot(p / ab, p / ab) > 1.0 {
        return d;
    } else {
        return -d;
    }
}

fn bezier_eval(p0: vec2f, p1: vec2f, p2: vec2f, p3: vec2f, t: f32) -> vec2f {
    let mt = 1.0 - t;
    return mt * mt * mt * p0
         + 3.0 * mt * mt * t * p1
         + 3.0 * mt * t * t * p2
         + t * t * t * p3;
}

fn bezier_tangent_eval(p0: vec2f, p1: vec2f, p2: vec2f, p3: vec2f, t: f32) -> vec2f {
    let mt = 1.0 - t;
    return 3.0 * mt * mt * (p1 - p0)
         + 6.0 * mt * t * (p2 - p1)
         + 3.0 * t * t * (p3 - p2);
}
