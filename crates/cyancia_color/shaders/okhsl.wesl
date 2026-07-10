import package::{
    xyz::Xyz,
    oklab::{OkLab, oklab_from_xyz, oklab_into_xyz},
    okcolor::{ChromaValues, chroma_values_from_normalized, st_mid, toe, toe_inv},
};

struct OkHsl {
    h: f32,
    s: f32,
    l: f32,
}

fn okhsl_from_xyz(xyz: Xyz) -> OkHsl {
    return okhsl_from_oklab(oklab_from_xyz(xyz));
}

fn okhsl_into_xyz(okhsl: OkHsl) -> Xyz {
    return oklab_into_xyz(okhsl_into_oklab(okhsl));
}

fn okhsl_from_oklab(oklab: OkLab) -> OkHsl {
    let lightness = toe(oklab.l);
    let chroma = length(vec2f(oklab.a, oklab.b));

    if chroma <= 0.0001 || oklab.l == 0.0 || oklab.l == 1.0 {
        return OkHsl(0.0, 0.0, lightness);
    }

    let hue = (degrees(atan2(oklab.b, oklab.a)) % 360.0 + 360.0) % 360.0;
    let a_ = oklab.a / chroma;
    let b_ = oklab.b / chroma;
    let chroma_vals = chroma_values_from_normalized(oklab.l, a_, b_);
    let mid = 0.8;
    let mid_inv = 1.25;

    let saturation = select(
        mid + (1.0 - mid) * (chroma - chroma_vals.mid)
            / (((1.0 - mid) * (chroma_vals.mid * mid_inv) * (chroma_vals.mid * mid_inv) / chroma_vals.zero)
            + (1.0 - ((1.0 - mid) * (chroma_vals.mid * mid_inv) * (chroma_vals.mid * mid_inv) / chroma_vals.zero) / (chroma_vals.max - chroma_vals.mid))
            * (chroma - chroma_vals.mid)),
        chroma / ((mid * chroma_vals.zero) + (1.0 - (mid * chroma_vals.zero) / chroma_vals.mid) * chroma) * mid,
        chroma < chroma_vals.mid,
    );

    return OkHsl(hue, saturation, lightness);
}

fn okhsl_into_oklab(okhsl: OkHsl) -> OkLab {
    if okhsl.l == 1.0 {
        return OkLab(1.0, 0.0, 0.0);
    }

    if okhsl.l == 0.0 {
        return OkLab(0.0, 0.0, 0.0);
    }

    let h = radians(okhsl.h);
    let a_ = cos(h);
    let b_ = sin(h);
    let lightness = toe_inv(okhsl.l);
    let chroma_vals = chroma_values_from_normalized(lightness, a_, b_);
    let mid = 0.8;
    let mid_inv = 1.25;

    let chroma = select(
        chroma_vals.mid
            + (okhsl.s - mid) / (1.0 - mid)
            * ((1.0 - mid) * chroma_vals.mid * chroma_vals.mid * mid_inv * mid_inv / chroma_vals.zero)
            / (1.0 - (1.0 - ((1.0 - mid) * chroma_vals.mid * chroma_vals.mid * mid_inv * mid_inv / chroma_vals.zero)
            / (chroma_vals.max - chroma_vals.mid))
            * (okhsl.s - mid) / (1.0 - mid)),
        (mid_inv * okhsl.s) * (mid * chroma_vals.zero)
            / (1.0 - (1.0 - (mid * chroma_vals.zero) / chroma_vals.mid) * (mid_inv * okhsl.s)),
        okhsl.s < mid,
    );

    return OkLab(lightness, chroma * a_, chroma * b_);
}
