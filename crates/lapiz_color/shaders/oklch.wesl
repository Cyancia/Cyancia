import package::{
    xyz::Xyz,
    oklab::{OkLab, oklab_from_xyz, oklab_into_xyz},
};

struct OkLch {
    l: f32,
    c: f32,
    h: f32,
}

fn oklch_from_xyz(xyz: Xyz) -> OkLch {
    return oklch_from_oklab(oklab_from_xyz(xyz));
}

fn oklch_into_xyz(oklch: OkLch) -> Xyz {
    return oklab_into_xyz(oklch_into_oklab(oklch));
}

fn oklch_from_oklab(oklab: OkLab) -> OkLch {
    let c = length(vec2f(oklab.a, oklab.b));
    let h = (degrees(atan2(oklab.b, oklab.a)) % 360.0 + 360.0) % 360.0;
    return OkLch(oklab.l, c, h);
}

fn oklch_into_oklab(oklch: OkLch) -> OkLab {
    let h = radians(oklch.h);
    return OkLab(oklch.l, oklch.c * cos(h), oklch.c * sin(h));
}
