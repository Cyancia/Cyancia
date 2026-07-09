use moxcms::Matrix3f;

use crate::model::xyz::Xyz;

/// Linear (scene-referred) RGB color in an arbitrary RGB working space.
///
/// `Rgb` stores **linear** light values — no gamma / transfer function is
/// applied. The color space (primaries, white point, tone curve) is NOT
/// stored on the struct; it is supplied at conversion time via the
/// `rgb_to_xyz` matrix parameter.
///
/// To convert to/from [`Xyz`], you must provide a 3×3 linear transformation
/// matrix specific to the RGB working space (e.g. sRGB → XYZ, Display P3 → XYZ).
///
/// - **r** — linear red, `[0.0, 1.0]` for display-referenced, unbounded for scene
/// - **g** — linear green, same range
/// - **b** — linear blue, same range
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rgb {
    /// Linear red component in `[0.0, 1.0]`.
    pub r: f32,
    /// Linear green component in `[0.0, 1.0]`.
    pub g: f32,
    /// Linear blue component in `[0.0, 1.0]`.
    pub b: f32,
}

impl Rgb {
    pub const fn new(r: f32, g: f32, b: f32) -> Self {
        Self { r, g, b }
    }

    pub fn from_xyz(xyz: Xyz, rgb_to_xyz: Matrix3f) -> Self {
        let rgb = moxcms::Xyz::new(xyz.x, xyz.y, xyz.z).to_linear_rgb(rgb_to_xyz.inverse());
        Self::new(rgb.r, rgb.g, rgb.b)
    }

    pub fn to_xyz(&self, rgb_to_xyz: Matrix3f) -> Xyz {
        let xyz =
            moxcms::Xyz::from_linear_rgb(moxcms::Rgb::new(self.r, self.g, self.b), rgb_to_xyz);
        Xyz::new(xyz.x, xyz.y, xyz.z)
    }
}

#[cfg(test)]
mod tests {
    use moxcms::ColorProfile;

    use crate::model::{rgb::Rgb, tests::roundtrip_test};

    #[test]
    fn roundtrip() {
        let profile = ColorProfile::new_srgb();

        roundtrip_test(
            Rgb::new,
            |rgb| rgb.to_xyz(profile.rgb_to_xyz_matrix().to_f32()),
            |xyz| Rgb::from_xyz(xyz, profile.rgb_to_xyz_matrix().to_f32()),
        );
    }
}
