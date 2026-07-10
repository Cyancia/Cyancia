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
    let src_lms = BRADFORD * vec3f(src_white.x, src_white.y, src_white.z);
    let dst_lms = BRADFORD * vec3f(dst_white.x, dst_white.y, dst_white.z);

    let ratio_diag = mat3x3f(
        vec3f(dst_lms.x / src_lms.x, 0.0, 0.0),
        vec3f(0.0, dst_lms.y / src_lms.y, 0.0),
        vec3f(0.0, 0.0, dst_lms.z / src_lms.z),
    );

    return INV_BRADFORD * ratio_diag * BRADFORD;
}

const BRADFORD: mat3x3f = mat3x3f(
    vec3f( 0.8951, -0.7502,  0.0389),
    vec3f( 0.2664,  1.7135, -0.0685),
    vec3f(-0.1614,  0.0367,  1.0296),
);

const INV_BRADFORD: mat3x3f = mat3x3f(
    vec3f(0.9869929,  0.4323053, -0.0085287),
    vec3f(-0.1470543, 0.5183603,  0.0400428),
    vec3f(0.1599627,  0.0492912,  0.9684867),
);
