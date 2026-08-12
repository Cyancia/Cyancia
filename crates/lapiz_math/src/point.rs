use iced_core::Point;

pub trait PointExt {
    fn magnitude_squared(&self) -> f64;
}

impl PointExt for Point {
    fn magnitude_squared(&self) -> f64 {
        (self.x * self.x + self.y * self.y) as f64
    }
}
