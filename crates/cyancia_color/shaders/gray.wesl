import package::xyz::Xyz;

struct Gray {
    v: f32,
}

fn gray_from_xyz(xyz: Xyz) -> Gray {
    return Gray(xyz.y);
}

fn gray_into_xyz(gray: Gray) -> Xyz {
    return Xyz(gray.v, gray.v, gray.v);
}
