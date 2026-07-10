import package::{
    xyz::Xyz,
    oklab::{OkLab, oklab_from_xyz, oklab_into_xyz},
    okcolor::{cbrt, LC, ST, find_cusp, st_from_lc, st_mid, oklab_to_linear_srgb, toe, toe_inv},
};

struct OkHsv {
    h: f32,
    s: f32,
    v: f32,
}

fn okhsv_from_xyz(xyz: Xyz) -> OkHsv {
    return okhsv_from_oklab(oklab_from_xyz(xyz));
}

fn okhsv_into_xyz(okhsv: OkHsv) -> Xyz {
    return oklab_into_xyz(okhsv_into_oklab(okhsv));
}

fn okhsv_from_oklab(oklab: OkLab) -> OkHsv {
    if oklab.l == 0.0 {
        return OkHsv(0.0, 0.0, 0.0);
    }

    let chroma = length(vec2f(oklab.a, oklab.b));
    if chroma <= 0.0001 {
        return OkHsv(0.0, 0.0, toe(oklab.l));
    }

    let a_ = oklab.a / chroma;
    let b_ = oklab.b / chroma;
    let hue = (degrees(atan2(oklab.b, oklab.a)) % 360.0 + 360.0) % 360.0;
    let cusp = find_cusp(a_, b_);
    let st_max = st_from_lc(cusp);
    let s_0 = 0.5;
    let k = 1.0 - s_0 / st_max.s;

    let t = st_max.t / (chroma + oklab.l * st_max.t);
    let l_v = t * oklab.l;
    let c_v = t * chroma;
    let l_vt = toe_inv(l_v);
    let c_vt = c_v * l_vt / l_v;
    let rgb_scale = oklab_to_linear_srgb(OkLab(l_vt, a_ * c_vt, b_ * c_vt));
    let lightness_scale_factor = cbrt(1.0 / max(max(rgb_scale.x, rgb_scale.y), max(rgb_scale.z, 0.0)));
    let l_r = toe(oklab.l / lightness_scale_factor);
    let v = l_r / l_v;
    let s = (s_0 + st_max.t) * c_v / (st_max.t * s_0 + st_max.t * k * c_v);

    return OkHsv(hue, s, v);
}

fn okhsv_into_oklab(okhsv: OkHsv) -> OkLab {
    if okhsv.v == 0.0 {
        return OkLab(0.0, 0.0, 0.0);
    }

    if okhsv.s == 0.0 {
        return OkLab(toe_inv(okhsv.v), 0.0, 0.0);
    }

    let h = radians(okhsv.h);
    let a_ = cos(h);
    let b_ = sin(h);
    let cusp = find_cusp(a_, b_);
    let st_cusp = st_from_lc(cusp);
    let s_0 = 0.5;
    let k = 1.0 - s_0 / st_cusp.s;

    let l_v = 1.0 - okhsv.s * s_0 / (s_0 + st_cusp.t - st_cusp.t * k * okhsv.s);
    let c_v = okhsv.s * st_cusp.t * s_0 / (s_0 + st_cusp.t - st_cusp.t * k * okhsv.s);
    let l_vt = toe_inv(l_v);
    let c_vt = c_v * l_vt / l_v;

    var lightness = okhsv.v * l_v;
    var chroma = okhsv.v * c_v;
    let lightness_new = toe_inv(lightness);
    chroma = chroma * lightness_new / lightness;

    let rgb_scale = oklab_to_linear_srgb(OkLab(l_vt, a_ * c_vt, b_ * c_vt));
    let lightness_scale_factor = cbrt(1.0 / max(max(rgb_scale.x, rgb_scale.y), max(rgb_scale.z, 0.0)));

    lightness = lightness_new * lightness_scale_factor;
    chroma = chroma * lightness_scale_factor;

    return OkLab(lightness, chroma * a_, chroma * b_);
}
