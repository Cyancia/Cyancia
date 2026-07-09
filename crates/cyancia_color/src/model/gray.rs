/// A single-channel grayscale value.
///
/// Represents a neutral (achromatic) luminance in `[0.0, 1.0]`.
/// The value is linear-light — no gamma or transfer function is applied.
///
/// Gray can be interpreted as the Y channel of [`Xyz`] (luminance) for
/// any white point, since neutral colors have X = Y = Z in relative XYZ.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Gray {
    /// Gray value in `[0.0, 1.0]`.
    pub v: f32,
}

impl Gray {
    pub const fn new(v: f32) -> Self {
        Self { v }
    }
}
