use cyancia_runtime::{Application, Services, event::Event, plugin::Plugin, service::Service};
use cyancia_utils::wrapper;
use moxcms::Matrix3f;

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

impl Color {
    pub fn into_rgb(self, xyz_to_rgb: Matrix3f) -> Rgb {
        match self {
            Color::Gray(gray) => Rgb::from_xyz(gray.into_xyz(), xyz_to_rgb),
            Color::Hsl(hsl) => hsl.into_rgb(),
            Color::Hsv(hsv) => hsv.into_rgb(),
            Color::Lab(lab) => Rgb::from_xyz(lab.into_xyz(), xyz_to_rgb),
            Color::Lch(lch) => Rgb::from_xyz(lch.into_xyz(), xyz_to_rgb),
            Color::OkHsl(ok_hsl) => Rgb::from_xyz(ok_hsl.into_xyz(), xyz_to_rgb),
            Color::OkHsv(ok_hsv) => Rgb::from_xyz(ok_hsv.into_xyz(), xyz_to_rgb),
            Color::OkLab(ok_lab) => Rgb::from_xyz(ok_lab.into_xyz(), xyz_to_rgb),
            Color::OkLch(ok_lch) => Rgb::from_xyz(ok_lch.into_xyz(), xyz_to_rgb),
            Color::Rgb(rgb) => rgb,
            Color::Xyz(xyz) => Rgb::from_xyz(xyz, xyz_to_rgb),
        }
    }
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

wrapper! {
    #[derive(Debug, Clone, Copy)]
    pub mut ForegroundColor : Color
}

impl Service for ForegroundColor {}

impl ForegroundColor {
    pub fn get(&self) -> Color {
        self.0
    }

    pub fn set(&mut self, color: Color) {
        self.0 = color;
    }
}

wrapper! {
    #[derive(Debug, Clone, Copy)]
    pub mut BackgroundColor : Color
}

impl BackgroundColor {
    pub fn get(&self) -> Color {
        self.0
    }

    pub fn set(&mut self, color: Color) {
        self.0 = color;
    }
}

impl Service for BackgroundColor {}

pub trait ForegroundBackgroundColorExt {
    fn foreground_color(&self) -> &ForegroundColor;
    fn foreground_color_mut(&mut self) -> &mut ForegroundColor;
    fn background_color(&self) -> &BackgroundColor;
    fn background_color_mut(&mut self) -> &mut BackgroundColor;
}

impl ForegroundBackgroundColorExt for Services {
    fn foreground_color(&self) -> &ForegroundColor {
        self.service::<ForegroundColor>()
    }

    fn foreground_color_mut(&mut self) -> &mut ForegroundColor {
        self.service_mut::<ForegroundColor>()
    }

    fn background_color(&self) -> &BackgroundColor {
        self.service::<BackgroundColor>()
    }

    fn background_color_mut(&mut self) -> &mut BackgroundColor {
        self.service_mut::<BackgroundColor>()
    }
}

#[derive(Event, Debug, Clone)]
pub struct ForegroundColorChanged {
    pub old: Color,
    pub new: Color,
}

impl ForegroundColorChanged {
    pub fn new(old: Color, new: Color) -> Self {
        Self { old, new }
    }
}

#[derive(Event, Debug, Clone)]
pub struct BackgroundColorChanged {
    pub old: Color,
    pub new: Color,
}

impl BackgroundColorChanged {
    pub fn new(old: Color, new: Color) -> Self {
        Self { old, new }
    }
}

pub struct ColorPlugin;

impl Plugin for ColorPlugin {
    fn build(&self, app: &mut Application) {
        let mut runtime = app.runtime_mut();
        let services = runtime.services_mut();

        let default_color = Color::Rgb(Rgb::new(0.0, 0.0, 0.0));
        services.insert_service(ForegroundColor::new(default_color));
        services.insert_service(BackgroundColor::new(default_color));
    }
}
