use gpui::{Pixels, Point};

pub trait PointExt {
    fn magnitude_squared(&self) -> f64;
}

impl PointExt for Point<Pixels> {
    fn magnitude_squared(&self) -> f64 {
        self.x.to_f64() * self.x.to_f64() + self.y.to_f64() * self.y.to_f64()
    }
}

impl PointExt for Point<f32> {
    fn magnitude_squared(&self) -> f64 {
        (self.x * self.x + self.y * self.y) as f64
    }
}

impl PointExt for Point<f64> {
    fn magnitude_squared(&self) -> f64 {
        self.x * self.x + self.y * self.y
    }
}
