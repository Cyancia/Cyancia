use glam::{IVec2, Mat3, UVec2, Vec2};
use iced_core::Rectangle;

macro_rules! impl_corners {
    ($trait_ident: ident, $data_ty: ty, $vec_ty: ident) => {
        pub trait $trait_ident {
            fn top_left(&self) -> $vec_ty;
            fn top_right(&self) -> $vec_ty;
            fn bottom_left(&self) -> $vec_ty;
            fn bottom_right(&self) -> $vec_ty;
        }

        impl $trait_ident for Rectangle<$data_ty> {
            fn top_left(&self) -> $vec_ty {
                $vec_ty {
                    x: self.x,
                    y: self.y,
                }
            }

            fn top_right(&self) -> $vec_ty {
                $vec_ty {
                    x: self.x + self.width,
                    y: self.y,
                }
            }

            fn bottom_left(&self) -> $vec_ty {
                $vec_ty {
                    x: self.x,
                    y: self.y + self.height,
                }
            }

            fn bottom_right(&self) -> $vec_ty {
                $vec_ty {
                    x: self.x + self.width,
                    y: self.y + self.height,
                }
            }
        }
    };
}

impl_corners!(URectangleCorners, u32, UVec2);
impl_corners!(IRectangleCorners, i32, IVec2);
impl_corners!(RectangleCorners, f32, Vec2);

pub trait RectangleTransform {
    fn transform(&self, mat: &Mat3) -> Rectangle;
}

impl RectangleTransform for Rectangle<u32> {
    fn transform(&self, mat: &Mat3) -> Rectangle {
        let tl = mat.transform_point2(self.top_left().as_vec2());
        let tr = mat.transform_point2(self.top_right().as_vec2());
        let bl = mat.transform_point2(self.bottom_left().as_vec2());
        let br = mat.transform_point2(self.bottom_right().as_vec2());

        let mn = tl.min(tr).min(bl).min(br).floor();
        let mx = tl.max(tr).max(bl).max(br).ceil();

        Rectangle {
            x: mn.x,
            y: mn.y,
            width: mx.x - mn.x,
            height: mx.y - mn.y,
        }
    }
}

impl RectangleTransform for Rectangle<i32> {
    fn transform(&self, mat: &Mat3) -> Rectangle {
        let tl = mat.transform_point2(self.top_left().as_vec2());
        let tr = mat.transform_point2(self.top_right().as_vec2());
        let bl = mat.transform_point2(self.bottom_left().as_vec2());
        let br = mat.transform_point2(self.bottom_right().as_vec2());

        let mn = tl.min(tr).min(bl).min(br).floor();
        let mx = tl.max(tr).max(bl).max(br).ceil();
        Rectangle {
            x: mn.x,
            y: mn.y,
            width: mx.x - mn.x,
            height: mx.y - mn.y,
        }
    }
}

impl RectangleTransform for Rectangle<f32> {
    fn transform(&self, mat: &Mat3) -> Rectangle {
        let tl = mat.transform_point2(self.top_left());
        let tr = mat.transform_point2(self.top_right());
        let bl = mat.transform_point2(self.bottom_left());
        let br = mat.transform_point2(self.bottom_right());

        let mn = tl.min(tr).min(bl).min(br).floor();
        let mx = tl.max(tr).max(bl).max(br).ceil();

        Rectangle {
            x: mn.x,
            y: mn.y,
            width: mx.x - mn.x,
            height: mx.y - mn.y,
        }
    }
}

pub trait URectangleConversion {
    fn as_irect(&self) -> Rectangle<i32>;
    fn as_frect(&self) -> Rectangle;
}

impl URectangleConversion for Rectangle<u32> {
    fn as_irect(&self) -> Rectangle<i32> {
        Rectangle {
            x: self.x as i32,
            y: self.y as i32,
            width: self.width as i32,
            height: self.height as i32,
        }
    }

    fn as_frect(&self) -> Rectangle {
        Rectangle {
            x: self.x as f32,
            y: self.y as f32,
            width: self.width as f32,
            height: self.height as f32,
        }
    }
}

pub trait IRectangleConversion {
    fn as_urect(&self) -> Rectangle<u32>;
    fn as_frect(&self) -> Rectangle;
}

impl IRectangleConversion for Rectangle<i32> {
    fn as_urect(&self) -> Rectangle<u32> {
        Rectangle {
            x: self.x as u32,
            y: self.y as u32,
            width: self.width as u32,
            height: self.height as u32,
        }
    }

    fn as_frect(&self) -> Rectangle {
        Rectangle {
            x: self.x as f32,
            y: self.y as f32,
            width: self.width as f32,
            height: self.height as f32,
        }
    }
}

pub trait RectangleConversion {
    fn as_urect(&self) -> Rectangle<u32>;
    fn as_irect(&self) -> Rectangle<i32>;
}

impl RectangleConversion for Rectangle {
    fn as_urect(&self) -> Rectangle<u32> {
        Rectangle {
            x: self.x as u32,
            y: self.y as u32,
            width: self.width as u32,
            height: self.height as u32,
        }
    }

    fn as_irect(&self) -> Rectangle<i32> {
        Rectangle {
            x: self.x as i32,
            y: self.y as i32,
            width: self.width as i32,
            height: self.height as i32,
        }
    }
}
