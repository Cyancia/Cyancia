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

fn blend_normal(src: vec4f, dst: vec4f) -> vec4f {
    return blend_compose(src, dst, src.rgb);
}

fn blend_additive(src: vec4f, dst: vec4f) -> vec4f {
    let blended_rgb = clamp01_rgb(src.rgb + dst.rgb);
    return blend_compose(src, dst, blended_rgb);
}

fn blend_subtractive(src: vec4f, dst: vec4f) -> vec4f {
    let blended_rgb = clamp01_rgb(dst.rgb - src.rgb);
    return blend_compose(src, dst, blended_rgb);
}

fn blend_multiply(src: vec4f, dst: vec4f) -> vec4f {
    let blended_rgb = src.rgb * dst.rgb;
    return blend_compose(src, dst, blended_rgb);
}

fn blend_divide(src: vec4f, dst: vec4f) -> vec4f {
    let blended_rgb = clamp01_rgb(dst.rgb / max(src.rgb, vec3f(BLEND_EPSILON)));
    return blend_compose(src, dst, blended_rgb);
}
