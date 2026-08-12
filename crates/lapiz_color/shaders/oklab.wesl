import package::{
    xyz::{Xyz, D50_WHITE, D65_WHITE, cat},
    okcolor::cbrt,
};

struct OkLab {
    l: f32,
    a: f32,
    b: f32,
}

fn oklab_from_xyz(xyz: Xyz) -> OkLab {
    return oklab_from_xyz_d65(cat(xyz, D50_WHITE, D65_WHITE));
}

fn oklab_into_xyz(oklab: OkLab) -> Xyz {
    return cat(oklab_into_xyz_d65(oklab), D65_WHITE, D50_WHITE);
}

fn oklab_from_xyz_d65(xyz: Xyz) -> OkLab {
    let l = 0.8189330101 * xyz.x + 0.3618667424 * xyz.y - 0.1288597137 * xyz.z;
    let m = 0.0329845436 * xyz.x + 0.9293118715 * xyz.y + 0.0361456387 * xyz.z;
    let s = 0.0482003018 * xyz.x + 0.2643662691 * xyz.y + 0.633851707 * xyz.z;

    let l_ = cbrt(l);
    let m_ = cbrt(m);
    let s_ = cbrt(s);

    return OkLab(
        0.2104542553 * l_ + 0.793617785 * m_ - 0.0040720468 * s_,
        1.9779984951 * l_ - 2.428592205 * m_ + 0.4505937099 * s_,
        0.0259040371 * l_ + 0.7827717662 * m_ - 0.808675766 * s_,
    );
}

fn oklab_into_xyz_d65(oklab: OkLab) -> Xyz {
    let l_ = oklab.l + 0.3963377774 * oklab.a + 0.2158037573 * oklab.b;
    let m_ = oklab.l - 0.1055613458 * oklab.a - 0.0638541728 * oklab.b;
    let s_ = oklab.l - 0.0894841775 * oklab.a - 1.291485548 * oklab.b;

    let l = l_ * l_ * l_;
    let m = m_ * m_ * m_;
    let s = s_ * s_ * s_;

    return Xyz(
        1.2270138511 * l - 0.5577999807 * m + 0.281256149 * s,
        -0.0405801784 * l + 1.1122568696 * m - 0.0716766787 * s,
        -0.0763812845 * l - 0.4214819784 * m + 1.5861632204 * s,
    );
}
