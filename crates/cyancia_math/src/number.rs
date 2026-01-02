pub trait AngleDifference {
    fn angle_difference(self, rhs: Self) -> Self;
}

impl AngleDifference for f32 {
    fn angle_difference(self, rhs: Self) -> Self {
        (self - rhs + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI
    }
}
