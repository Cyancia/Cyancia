use cyancia_runtime::{Application, Services, event::Event, plugin::Plugin, service::Service};
use cyancia_utils::wrapper;

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

wrapper! {
    #[derive(Debug, Clone, Copy)]
    pub ForegroundColor : Color
}

impl Service for ForegroundColor {}

wrapper! {
    #[derive(Debug, Clone, Copy)]
    pub BackgroundColor : Color
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

#[derive(Event, Debug, Clone)]
pub struct BackgroundColorChanged {
    pub old: Color,
    pub new: Color,
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
