import package::oklab::OkLab;

struct LC {
    lightness: f32,
    chroma: f32,
}

struct ST {
    s: f32,
    t: f32,
}

struct ChromaValues {
    zero: f32,
    mid: f32,
    max: f32,
}

fn cbrt(x: f32) -> f32 {
    return sign(x) * pow(abs(x), 1.0 / 3.0);
}

fn toe(oklab_lightness: f32) -> f32 {
    let k_1 = 0.206;
    let k_2 = 0.03;
    let k_3 = (1.0 + k_1) / (1.0 + k_2);
    return 0.5 * (k_3 * oklab_lightness - k_1
        + sqrt((k_3 * oklab_lightness - k_1) * (k_3 * oklab_lightness - k_1) + 4.0 * k_2 * k_3 * oklab_lightness));
}

fn toe_inv(l_r: f32) -> f32 {
    let k_1 = 0.206;
    let k_2 = 0.03;
    let k_3 = (1.0 + k_1) / (1.0 + k_2);
    return (l_r * l_r + k_1 * l_r) / (k_3 * (l_r + k_2));
}

fn find_cusp(a: f32, b: f32) -> LC {
    let max_sat = max_saturation(a, b);
    let rgb_at_max = oklab_to_linear_srgb(OkLab(1.0, max_sat * a, max_sat * b));
    let max_lightness = cbrt(1.0 / max(rgb_at_max.x, max(rgb_at_max.y, rgb_at_max.z)));

    return LC(max_lightness, max_lightness * max_sat);
}

fn oklab_to_linear_srgb(oklab: OkLab) -> vec3f {
    let l_ = oklab.l + 0.3963377774 * oklab.a + 0.2158037573 * oklab.b;
    let m_ = oklab.l - 0.1055613458 * oklab.a - 0.0638541728 * oklab.b;
    let s_ = oklab.l - 0.0894841775 * oklab.a - 1.291485548 * oklab.b;
    let l = l_ * l_ * l_;
    let m = m_ * m_ * m_;
    let s = s_ * s_ * s_;

    return vec3f(
        4.0767417 * l - 3.3077116 * m + 0.23096994 * s,
        -1.268438 * l + 2.6097574 * m - 0.3413194 * s,
        -0.0041960863 * l - 0.7034186 * m + 1.7076147 * s,
    );
}

fn chroma_values_from_normalized(lightness: f32, a_: f32, b_: f32) -> ChromaValues {
    let cusp = find_cusp(a_, b_);
    let max_chroma = find_gamut_intersection(a_, b_, lightness, 1.0, lightness, cusp);
    let st_max = st_from_lc(cusp);
    let k = max_chroma / min(lightness * st_max.s, (1.0 - lightness) * st_max.t);
    let st_mid_val = st_mid(a_, b_);

    let c_a = lightness * st_mid_val.s;
    let c_b = (1.0 - lightness) * st_mid_val.t;
    let c_mid = 0.9
        * k
        * sqrt(sqrt(1.0 / (1.0 / (c_a * c_a * c_a * c_a) + 1.0 / (c_b * c_b * c_b * c_b))));

    let c0_a = lightness * 0.4;
    let c0_b = (1.0 - lightness) * 0.8;
    let c_0 = sqrt(1.0 / (1.0 / (c0_a * c0_a) + 1.0 / (c0_b * c0_b)));

    return ChromaValues(c_0, c_mid, max_chroma);
}

fn st_from_lc(lc: LC) -> ST {
    return ST(lc.chroma / lc.lightness, lc.chroma / (1.0 - lc.lightness));
}

fn st_mid(a_: f32, b_: f32) -> ST {
    let s = 0.11516993
        + 1.0
            / (7.4477897
                + 4.1590123 * b_
                + a_ * (-2.1955736
                    + 1.751984 * b_
                    + a_ * (-2.1370494 - 10.02301 * b_
                        + a_ * (-4.248946 + 5.387708 * b_ + 4.6989102 * a_))));
    let t = 0.11239642
        + 1.0
            / (1.6132032 - 0.6812438 * b_
                + a_ * (0.40370613
                    + 0.9014812 * b_
                    + a_ * (-0.27087942
                        + 0.6122399 * b_
                        + a_ * (0.00299215 - 0.45399567 * b_ - 0.14661872 * a_))));
    return ST(s, t);
}

fn find_gamut_intersection(a: f32, b: f32, l1: f32, c1: f32, l0: f32, cusp: LC) -> f32 {
    if (l1 - l0) * cusp.chroma - (cusp.lightness - l0) * c1 <= 0.0 {
        return cusp.chroma * l0 / (c1 * cusp.lightness + cusp.chroma * (l0 - l1));
    }

    let t = cusp.chroma * (l0 - 1.0) / (c1 * (cusp.lightness - 1.0) + cusp.chroma * (l0 - l1));
    let dl = l1 - l0;
    let dc = c1;
    let k_l = 0.39633778 * a + 0.21580376 * b;
    let k_m = -0.105561346 * a - 0.06385417 * b;
    let k_s = -0.08948418 * a - 1.2914855 * b;
    let l_dt = dl + dc * k_l;
    let m_dt = dl + dc * k_m;
    let s_dt = dl + dc * k_s;

    let lightness = l0 * (1.0 - t) + t * l1;
    let chroma = t * c1;
    let l_ = lightness + chroma * k_l;
    let m_ = lightness + chroma * k_m;
    let s_ = lightness + chroma * k_s;

    let l = l_ * l_ * l_;
    let m = m_ * m_ * m_;
    let s = s_ * s_ * s_;

    let ldt = 3.0 * l_dt * l_ * l_;
    let mdt = 3.0 * m_dt * m_ * m_;
    let sdt = 3.0 * s_dt * s_ * s_;

    let ldt2 = 6.0 * l_dt * l_dt * l_;
    let mdt2 = 6.0 * m_dt * m_dt * m_;
    let sdt2 = 6.0 * s_dt * s_dt * s_;

    let t_r = halley_step(
        4.0767417 * l - 3.3077116 * m + 0.23096994 * s - 1.0,
        4.0767417 * ldt - 3.3077116 * mdt + 0.23096994 * sdt,
        4.0767417 * ldt2 - 3.3077116 * mdt2 + 0.23096994 * sdt2,
    );
    let t_g = halley_step(
        -1.268438 * l + 2.6097574 * m - 0.3413194 * s - 1.0,
        -1.268438 * ldt + 2.6097574 * mdt - 0.3413194 * sdt,
        -1.268438 * ldt2 + 2.6097574 * mdt2 - 0.3413194 * sdt2,
    );
    let t_b = halley_step(
        -0.0041960863 * l - 0.7034186 * m + 1.7076147 * s - 1.0,
        -0.0041960863 * ldt - 0.7034186 * mdt + 1.7076147 * sdt,
        -0.0041960863 * ldt2 - 0.7034186 * mdt2 + 1.7076147 * sdt2,
    );

    return t + min(t_r, min(t_g, t_b));
}

fn halley_step(f: f32, f1: f32, f2: f32) -> f32 {
    let u = f1 / (f1 * f1 - 0.5 * f * f2);
    if u >= 0.0 {
        return -f * u;
    } else {
        return 1000000.0;
    }
}

fn max_saturation(a: f32, b: f32) -> f32 {
    var k0: f32;
    var k1: f32;
    var k2: f32;
    var k3: f32;
    var k4: f32;
    var wl: f32;
    var wm: f32;
    var ws: f32;

    if -1.8817033 * a - 0.8093649 * b > 1.0 {
        k0 = 1.1908628;
        k1 = 1.7657673;
        k2 = 0.5966264;
        k3 = 0.755152;
        k4 = 0.5677124;
        wl = 4.0767417;
        wm = -3.3077116;
        ws = 0.23096994;
    } else if 1.814441 * a - 1.1944528 * b > 1.0 {
        k0 = 0.73956513;
        k1 = -0.45954403;
        k2 = 0.08285427;
        k3 = 0.1254107;
        k4 = 0.14503205;
        wl = -1.268438;
        wm = 2.6097574;
        ws = -0.3413194;
    } else {
        k0 = 1.3573365;
        k1 = -0.00915799;
        k2 = -1.1513021;
        k3 = -0.50559604;
        k4 = 0.00692167;
        wl = -0.0041960863;
        wm = -0.7034186;
        ws = 1.7076147;
    }

    var saturation = k0 + k1 * a + k2 * b + k3 * a * a + k4 * a * b;
    let k_l = 0.39633778 * a + 0.21580376 * b;
    let k_m = -0.105561346 * a - 0.06385417 * b;
    let k_s = -0.08948418 * a - 1.2914855 * b;

    let l_ = 1.0 + saturation * k_l;
    let m_ = 1.0 + saturation * k_m;
    let s_ = 1.0 + saturation * k_s;
    let l = l_ * l_ * l_;
    let m = m_ * m_ * m_;
    let s = s_ * s_ * s_;

    let l_ds = 3.0 * k_l * l_ * l_;
    let m_ds = 3.0 * k_m * m_ * m_;
    let s_ds = 3.0 * k_s * s_ * s_;

    let l_ds2 = 6.0 * k_l * k_l * l_;
    let m_ds2 = 6.0 * k_m * k_m * m_;
    let s_ds2 = 6.0 * k_s * k_s * s_;

    let f = wl * l + wm * m + ws * s;
    let f1 = wl * l_ds + wm * m_ds + ws * s_ds;
    let f2 = wl * l_ds2 + wm * m_ds2 + ws * s_ds2;

    saturation = saturation - f * f1 / (f1 * f1 - 0.5 * f * f2);
    return saturation;
}
