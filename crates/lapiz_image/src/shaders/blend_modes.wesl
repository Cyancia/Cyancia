const BLEND_EPSILON: f32 = 1e-6;

fn clamp01_rgb(color: vec3f) -> vec3f {
    return clamp(color, vec3f(0.0), vec3f(1.0));
}

fn blend_compose(src: vec4f, dst: vec4f, blended_rgb: vec3f) -> vec4f {
    let src_a = clamp(src.a, 0.0, 1.0);
    let dst_a = clamp(dst.a, 0.0, 1.0);
    let out_a = src_a + dst_a * (1.0 - src_a);

    if out_a <= BLEND_EPSILON {
        return vec4f(0.0);
    }

    let out_rgb_premul =
        src.rgb * src_a * (1.0 - dst_a) +
        dst.rgb * dst_a * (1.0 - src_a) +
        blended_rgb * src_a * dst_a;
    let out_rgb = out_rgb_premul / out_a;

    return vec4f(clamp01_rgb(out_rgb), out_a);
}

fn lum(c: vec3f) -> f32 {
    return dot(c, vec3f(0.3, 0.59, 0.11));
}

fn clip_color(c: vec3f) -> vec3f {
    let l = lum(c);
    let n = min(c.r, min(c.g, c.b));
    let x = max(c.r, max(c.g, c.b));
    var result = c;
    if n < 0.0 {
        result = l + (result - l) * l / (l - n);
    }
    if x > 1.0 {
        result = l + (result - l) * (1.0 - l) / (x - l);
    }
    return result;
}

fn set_lum(c: vec3f, target_lum: f32) -> vec3f {
    return clip_color(c + vec3f(target_lum - lum(c)));
}

fn sat(c: vec3f) -> f32 {
    return max(c.r, max(c.g, c.b)) - min(c.r, min(c.g, c.b));
}

fn set_sat(c: vec3f, target_sat: f32) -> vec3f {
    let c_min = min(c.r, min(c.g, c.b));
    let c_max = max(c.r, max(c.g, c.b));
    if c_max <= c_min {
        return vec3f(0.0);
    }
    return (c - vec3f(c_min)) / (c_max - c_min) * target_sat;
}

fn blend_normal(src: vec4f, dst: vec4f) -> vec4f {
    return blend_compose(src, dst, src.rgb);
}

fn blend_darken(src: vec4f, dst: vec4f) -> vec4f {
    return blend_compose(src, dst, min(src.rgb, dst.rgb));
}

fn blend_multiply(src: vec4f, dst: vec4f) -> vec4f {
    return blend_compose(src, dst, src.rgb * dst.rgb);
}

fn blend_color_burn(src: vec4f, dst: vec4f) -> vec4f {
    let s = src.rgb;
    let d = dst.rgb;
    var b = vec3f(1.0) - min(vec3f(1.0), (vec3f(1.0) - d) / max(s, vec3f(BLEND_EPSILON)));
    b = select(b, vec3f(1.0), d >= vec3f(1.0));
    b = select(b, vec3f(0.0), all(s <= vec3f(0.0)) && all(d < vec3f(1.0)));
    return blend_compose(src, dst, b);
}

fn blend_linear_burn(src: vec4f, dst: vec4f) -> vec4f {
    return blend_compose(src, dst, clamp01_rgb(dst.rgb + src.rgb - vec3f(1.0)));
}

fn blend_darker_color(src: vec4f, dst: vec4f) -> vec4f {
    let src_total = src.r + src.g + src.b;
    let dst_total = dst.r + dst.g + dst.b;
    return blend_compose(src, dst, select(dst.rgb, src.rgb, src_total < dst_total));
}

fn blend_lighten(src: vec4f, dst: vec4f) -> vec4f {
    return blend_compose(src, dst, max(src.rgb, dst.rgb));
}

fn blend_screen(src: vec4f, dst: vec4f) -> vec4f {
    return blend_compose(src, dst, dst.rgb + src.rgb - dst.rgb * src.rgb);
}

fn blend_color_dodge(src: vec4f, dst: vec4f) -> vec4f {
    let s = src.rgb;
    let d = dst.rgb;
    var b = min(vec3f(1.0), d / max(vec3f(1.0) - s, vec3f(BLEND_EPSILON)));
    b = select(b, vec3f(0.0), d <= vec3f(0.0));
    b = select(b, vec3f(1.0), all(s >= vec3f(1.0)) & all(d > vec3f(0.0)));
    return blend_compose(src, dst, b);
}

fn blend_linear_dodge(src: vec4f, dst: vec4f) -> vec4f {
    return blend_compose(src, dst, clamp01_rgb(src.rgb + dst.rgb));
}

fn blend_additive(src: vec4f, dst: vec4f) -> vec4f {
    return blend_linear_dodge(src, dst);
}

fn blend_lighter_color(src: vec4f, dst: vec4f) -> vec4f {
    let src_total = src.r + src.g + src.b;
    let dst_total = dst.r + dst.g + dst.b;
    return blend_compose(src, dst, select(dst.rgb, src.rgb, src_total > dst_total));
}

fn blend_overlay(src: vec4f, dst: vec4f) -> vec4f {
    let s = src.rgb;
    let d = dst.rgb;
    let mask = d > vec3f(0.5);
    let mul = d * s * vec3f(2.0);
    let scr = vec3f(1.0) - vec3f(2.0) * (vec3f(1.0) - d) * (vec3f(1.0) - s);
    return blend_compose(src, dst, select(mul, scr, mask));
}

fn blend_soft_light(src: vec4f, dst: vec4f) -> vec4f {
    let s = src.rgb;
    let d = dst.rgb;
    let mask = s > vec3f(0.5);
    let d_sq = d * d;
    let d2 = d * vec3f(2.0);
    let d2m = d2 - vec3f(1.0);
    let lower = d - (vec3f(1.0) - s * vec3f(2.0)) * d * (vec3f(1.0) - d);
    let d_func = select(sqrt(d), ((d * vec3f(16.0) - vec3f(12.0)) * d + vec3f(4.0)) * d, d <= vec3f(0.25));
    let upper = d + (s * vec3f(2.0) - vec3f(1.0)) * (d_func - d);
    return blend_compose(src, dst, select(lower, upper, mask));
}

fn blend_hard_light(src: vec4f, dst: vec4f) -> vec4f {
    let s = src.rgb;
    let d = dst.rgb;
    let mask = s > vec3f(0.5);
    let mul = d * s * vec3f(2.0);
    let scr = vec3f(1.0) - vec3f(2.0) * (vec3f(1.0) - d) * (vec3f(1.0) - s);
    return blend_compose(src, dst, select(mul, scr, mask));
}

fn blend_vivid_light(src: vec4f, dst: vec4f) -> vec4f {
    let s = src.rgb;
    let d = dst.rgb;
    let mask = s > vec3f(0.5);
    let s2 = s * vec3f(2.0);
    let burn = vec3f(1.0) - min(vec3f(1.0), (vec3f(1.0) - d) / max(s2, vec3f(BLEND_EPSILON)));
    let dodge = min(vec3f(1.0), d / max(vec3f(1.0) - (s2 - vec3f(1.0)), vec3f(BLEND_EPSILON)));
    return blend_compose(src, dst, select(burn, dodge, mask));
}

fn blend_linear_light(src: vec4f, dst: vec4f) -> vec4f {
    return blend_compose(src, dst, clamp01_rgb(dst.rgb + src.rgb * vec3f(2.0) - vec3f(1.0)));
}

fn blend_pin_light(src: vec4f, dst: vec4f) -> vec4f {
    let s = src.rgb;
    let d = dst.rgb;
    let mask = s > vec3f(0.5);
    let dark_part = min(d, s * vec3f(2.0));
    let light_part = max(d, s * vec3f(2.0) - vec3f(1.0));
    return blend_compose(src, dst, select(dark_part, light_part, mask));
}

fn blend_hard_mix(src: vec4f, dst: vec4f) -> vec4f {
    return blend_compose(src, dst, select(vec3f(0.0), vec3f(1.0), dst.rgb + src.rgb >= vec3f(1.0)));
}

fn blend_difference(src: vec4f, dst: vec4f) -> vec4f {
    return blend_compose(src, dst, abs(dst.rgb - src.rgb));
}

fn blend_exclusion(src: vec4f, dst: vec4f) -> vec4f {
    return blend_compose(src, dst, dst.rgb + src.rgb - vec3f(2.0) * dst.rgb * src.rgb);
}

fn blend_subtract(src: vec4f, dst: vec4f) -> vec4f {
    return blend_compose(src, dst, clamp01_rgb(dst.rgb - src.rgb));
}

fn blend_subtractive(src: vec4f, dst: vec4f) -> vec4f {
    return blend_subtract(src, dst);
}

fn blend_divide(src: vec4f, dst: vec4f) -> vec4f {
    return blend_compose(src, dst, clamp01_rgb(dst.rgb / max(src.rgb, vec3f(BLEND_EPSILON))));
}

fn blend_hue(src: vec4f, dst: vec4f) -> vec4f {
    return blend_compose(src, dst, set_lum(set_sat(src.rgb, sat(dst.rgb)), lum(dst.rgb)));
}

fn blend_saturation(src: vec4f, dst: vec4f) -> vec4f {
    return blend_compose(src, dst, set_lum(set_sat(dst.rgb, sat(src.rgb)), lum(dst.rgb)));
}

fn blend_color(src: vec4f, dst: vec4f) -> vec4f {
    return blend_compose(src, dst, set_lum(src.rgb, lum(dst.rgb)));
}

fn blend_luminosity(src: vec4f, dst: vec4f) -> vec4f {
    return blend_compose(src, dst, set_lum(dst.rgb, lum(src.rgb)));
}
