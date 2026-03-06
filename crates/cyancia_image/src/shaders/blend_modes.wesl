fn blend_normal(src: vec4f, dst: vec4f) -> vec4f {
    let a = src.a + dst.a * (1.0 - src.a);
    let rgb = (src.rgb * src.a + dst.rgb * dst.a * (1.0 - src.a)) / a;
    return vec4f(rgb, a);
}

fn blend_additive(src: vec4f, dst: vec4f) -> vec4f {
    return vec4f(mix(dst.rgb, dst.rgb + src.rgb, src.a), dst.a);
}

fn blend_subtractive(src: vec4f, dst: vec4f) -> vec4f {
    return vec4f(mix(dst.rgb, dst.rgb - src.rgb, src.a), dst.a);
}

fn blend_multiply(src: vec4f, dst: vec4f) -> vec4f {
    return vec4f(mix(dst.rgb, dst.rgb * src.rgb, src.a), dst.a);
}

fn blend_divide(src: vec4f, dst: vec4f) -> vec4f {
    return vec4f(mix(dst.rgb, dst.rgb / src.rgb, src.a), dst.a);
}
