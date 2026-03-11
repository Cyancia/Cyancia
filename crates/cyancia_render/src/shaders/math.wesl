struct Rect {
    min: vec2f,
    max: vec2f,
}

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
