import package::{xyz::Xyz, lab::{Lab, lab_from_xyz, lab_into_xyz}};

struct Lch {
    l: f32,
    c: f32,
    h: f32,
}

fn lch_from_xyz(xyz: Xyz) -> Lch {
    return lch_from_lab(lab_from_xyz(xyz));
}

fn lch_into_xyz(lch: Lch) -> Xyz {
    return lab_into_xyz(lch_into_lab(lch));
}

fn lch_from_lab(lab: Lab) -> Lch {
    let c = length(vec2f(lab.a, lab.b));
    let h = (degrees(atan2(lab.b, lab.a)) % 360.0 + 360.0) % 360.0;
    return Lch(lab.l, c, h);
}

fn lch_into_lab(lch: Lch) -> Lab {
    let h = radians(lch.h);
    return Lab(lch.l, lch.c * cos(h), lch.c * sin(h));
}
