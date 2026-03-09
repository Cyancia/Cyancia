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
    let p1 = mat * vec3f(vec2f(rect.max.x, rect.min.y), 1.0);
    let p2 = mat * vec3f(vec2f(rect.min.x, rect.max.y), 1.0);
    let p3 = mat * vec3f(rect.max, 1.0);
    let min = min(min(p0.xy, p1.xy), min(p2.xy, p3.xy));
    let max = max(max(p0.xy, p1.xy), max(p2.xy, p3.xy));
    return Rect(min, max);
}
