fn rgba8_coord(pixel: vec2u) -> vec2u {
    return pixel;
}

fn unpack_rgba8_texel(texel: vec4u) -> vec4f {
    return unpack_rgba8(texel.r);
}

fn pack_rgba8_texel(color: vec4f) -> vec4u {
    return vec4u(pack_rgba8(color), 0, 0, 0);
}

fn unpack_rgba8(x: u32) -> vec4f {
    let r = f32((x >> 0) & 0xFF) / 255.0;
    let g = f32((x >> 8) & 0xFF) / 255.0;
    let b = f32((x >> 16) & 0xFF) / 255.0;
    let a = f32((x >> 24) & 0xFF) / 255.0;
    return vec4f(r, g, b, a);
}

fn pack_rgba8(color: vec4f) -> u32 {
    let r = u32(clamp(color.r * 255.0, 0.0, 255.0));
    let g = u32(clamp(color.g * 255.0, 0.0, 255.0));
    let b = u32(clamp(color.b * 255.0, 0.0, 255.0));
    let a = u32(clamp(color.a * 255.0, 0.0, 255.0));
    return (a << 24) | (b << 16) | (g << 8) | r;
}
