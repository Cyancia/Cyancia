struct Xyz {
    x: f32,
    y: f32,
    z: f32,
}

const D50_WHITE: Xyz = Xyz(0.96422, 1.0, 0.82521);
const D65_WHITE: Xyz = Xyz(0.95047, 1.0, 1.08883);

fn cat(xyz: Xyz, src_white: Xyz, dst_white: Xyz) -> Xyz {
    let adapted = cat_matrix(src_white, dst_white) * vec3f(xyz.x, xyz.y, xyz.z);
    return Xyz(adapted.x, adapted.y, adapted.z);
}

fn cat_matrix(src_white: Xyz, dst_white: Xyz) -> mat3x3f {
    let bradford = mat3x3f(
        vec3f(0.8951, -0.7502, 0.0389),
        vec3f(0.2664, 1.7135, -0.0685),
        vec3f(-0.1614, 0.0367, 1.0296),
    );

    let src_lms = bradford * vec3f(src_white.x, src_white.y, src_white.z);
    let dst_lms = bradford * vec3f(dst_white.x, dst_white.y, dst_white.z);

    let sx = dst_lms.x / src_lms.x;
    let sy = dst_lms.y / src_lms.y;
    let sz = dst_lms.z / src_lms.z;

    // scale * bradford (scale is diagonal, so it scales each row of bradford)
    let scaled = mat3x3f(
        vec3f(sx * 0.8951, sy * -0.7502, sz * 0.0389),
        vec3f(sx * 0.2664, sy * 1.7135, sz * -0.0685),
        vec3f(sx * -0.1614, sy * 0.0367, sz * 1.0296),
    );

    return inverse_mat3x3(bradford) * scaled;
}

fn inverse_mat3x3(matrix: mat3x3f) -> mat3x3f {
    let tmp0 = cross(matrix[1], matrix[2]);
    let tmp1 = cross(matrix[2], matrix[0]);
    let tmp2 = cross(matrix[0], matrix[1]);
    let inv_det = 1.0 / dot(matrix[2], tmp2);
    return transpose(mat3x3f(tmp0 * inv_det, tmp1 * inv_det, tmp2 * inv_det));
}
