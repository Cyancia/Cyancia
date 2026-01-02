use glam::{Mat3, UVec2, Vec2};

#[derive(Default, Debug, Clone)]
pub struct CanvasTransform {
    pub widget_size: Vec2,
    pub pixel_to_widget: Mat3,
}

impl CanvasTransform {
    pub fn translate(&mut self, delta: Vec2) {
        let translation = Mat3::from_translation(delta);
        self.pixel_to_widget = translation * self.pixel_to_widget;
    }

    pub fn rotate_around(&mut self, angle: f32, center_ws: Vec2) {
        let new_mat = Mat3::from_translation(center_ws)
            * Mat3::from_angle(angle)
            * Mat3::from_translation(-center_ws)
            * self.pixel_to_widget;
        self.pixel_to_widget = new_mat;
    }

    pub fn scale_around(&mut self, scale_factor: f32, center_ws: Vec2) {
        let new_mat = Mat3::from_translation(center_ws)
            * Mat3::from_scale(Vec2::splat(scale_factor))
            * Mat3::from_translation(-center_ws)
            * self.pixel_to_widget;
        self.pixel_to_widget = new_mat;
    }

    pub fn translated(mut self, delta: Vec2) -> Self {
        self.translate(delta);
        self
    }

    pub fn rotated_around(mut self, angle: f32, center_ws: Vec2) -> Self {
        self.rotate_around(angle, center_ws);
        self
    }

    pub fn scaled_around(mut self, scale_factor: f32, center_ws: Vec2) -> Self {
        self.scale_around(scale_factor, center_ws);
        self
    }
}
