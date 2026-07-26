import package::xyz::Xyz;

struct Rgb {
    r: f32,
    g: f32,
    b: f32,
}

fn rgb_from_xyz(xyz: Xyz, xyz_to_rgb: mat3x3f) -> Rgb {
    let rgb = xyz_to_rgb * vec3f(xyz.x, xyz.y, xyz.z);
    return Rgb(rgb.x, rgb.y, rgb.z);
}

fn rgb_into_xyz(rgb: Rgb, rgb_to_xyz: mat3x3f) -> Xyz {
    let xyz = rgb_to_xyz * vec3f(rgb.r, rgb.g, rgb.b);
    return Xyz(xyz.x, xyz.y, xyz.z);
}
