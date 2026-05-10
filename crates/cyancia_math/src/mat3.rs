use glam::{Mat3, Vec2};

pub trait Mat3ScaleRotattionTranslationWithAnchor {
    fn from_scale_angle_translation_with_anchor(
        scale: Vec2,
        angle: f32,
        translation: Vec2,
        anchor: Vec2,
    ) -> Self;
}

impl Mat3ScaleRotattionTranslationWithAnchor for Mat3 {
    fn from_scale_angle_translation_with_anchor(
        scale: Vec2,
        angle: f32,
        translation: Vec2,
        anchor: Vec2,
    ) -> Self {
        let (s, c) = angle.sin_cos();

        let m00 = scale.x * c;
        let m10 = scale.x * s;
        let m01 = -scale.y * s;
        let m11 = scale.y * c;

        let m02 = translation.x - (anchor.x * m00 + anchor.y * m01);
        let m12 = translation.y - (anchor.x * m10 + anchor.y * m11);

        Self::from_cols_array(&[m00, m10, 0.0, m01, m11, 0.0, m02, m12, 1.0])
    }
}
