import package::xyz::{Xyz, D50_WHITE};

struct Lab {
    l: f32,
    a: f32,
    b: f32,
}

const LAB_EPSILON: f32 = 0.008856451679035631; // 216.0 / 24389.0
const LAB_KAPPA: f32 = 903.2962962962963;       // 24389.0 / 27.0

fn lab_f(t: f32) -> f32 {
    if t > LAB_EPSILON {
        return pow(t, 1.0 / 3.0);
    } else {
        return (LAB_KAPPA * t + 16.0) / 116.0;
    }
}

fn lab_f_inv(t: f32) -> f32 {
    let t3 = t * t * t;
    if t3 > LAB_EPSILON {
        return t3;
    } else {
        return (116.0 * t - 16.0) / LAB_KAPPA;
    }
}

fn lab_from_xyz(xyz: Xyz) -> Lab {
    return lab_from_xyz_with(xyz, D50_WHITE);
}

fn lab_into_xyz(lab: Lab) -> Xyz {
    return lab_into_xyz_with(lab, D50_WHITE);
}

fn lab_from_xyz_with(xyz: Xyz, white: Xyz) -> Lab {
    let x = lab_f(xyz.x / white.x);
    let y = lab_f(xyz.y / white.y);
    let z = lab_f(xyz.z / white.z);

    return Lab(116.0 * y - 16.0, 500.0 * (x - y), 200.0 * (y - z));
}

fn lab_into_xyz_with(lab: Lab, white: Xyz) -> Xyz {
    let y = (lab.l + 16.0) / 116.0;
    let x = y + lab.a / 500.0;
    let z = y - lab.b / 200.0;

    return Xyz(
        white.x * lab_f_inv(x),
        white.y * lab_f_inv(y),
        white.z * lab_f_inv(z),
    );
}
