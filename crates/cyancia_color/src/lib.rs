use crate::model::{
    gray::Gray, hsl::Hsl, hsv::Hsv, lab::Lab, lch::Lch, okhsl::OkHsl, okhsv::OkHsv, oklab::OkLab,
    oklch::OkLch, rgb::Rgb, xyz::Xyz,
};

wesl::wesl_pkg!(pub color);

pub mod model;
pub mod platform;
pub mod shader;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Color {
    Gray(Gray),
    Hsl(Hsl),
    Hsv(Hsv),
    Lab(Lab),
    Lch(Lch),
    OkHsl(OkHsl),
    OkHsv(OkHsv),
    OkLab(OkLab),
    OkLch(OkLch),
    Rgb(Rgb),
    Xyz(Xyz),
}

macro_rules! impl_from {
    ($variant:ident, $ty:ty) => {
        impl From<$ty> for Color {
            fn from(value: $ty) -> Self {
                Color::$variant(value)
            }
        }
    };
}
impl_from!(Gray, Gray);
impl_from!(Hsl, Hsl);
impl_from!(Hsv, Hsv);
impl_from!(Lab, Lab);
impl_from!(Lch, Lch);
impl_from!(OkHsl, OkHsl);
impl_from!(OkHsv, OkHsv);
impl_from!(OkLab, OkLab);
impl_from!(OkLch, OkLch);
impl_from!(Rgb, Rgb);
impl_from!(Xyz, Xyz);
