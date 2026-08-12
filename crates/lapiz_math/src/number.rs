pub trait AngleDifference {
    fn angle_difference(self, rhs: Self) -> Self;
}

impl AngleDifference for f32 {
    fn angle_difference(self, rhs: Self) -> Self {
        (self - rhs + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI
    }
}

pub trait LerpAngle {
    fn lerp_angle(self, rhs: Self, t: f32) -> Self;
}

impl LerpAngle for f32 {
    fn lerp_angle(self, rhs: Self, t: f32) -> Self {
        let diff = (rhs - self) % std::f32::consts::TAU;
        let shortest = (2.0 * diff) % std::f32::consts::TAU - diff;
        self + shortest * t
    }
}
