use cyancia_widgets::spin_slider::SpinSlider;
use glam::{Vec2, Vec3};
use iced_core::{Color, Element, color};
use iced_widget::column;

use crate::{GraphRenderer, GraphTheme, graph::slot::GraphValueType};

#[derive(Default, Clone)]
pub struct F32Type;

impl GraphValueType for F32Type {
    type AssociatedLiteralType = f32;

    type Message = f32;

    fn color(&self) -> Color {
        color!(0x0A9F8D)
    }

    fn name(&self) -> &'static str {
        "Float"
    }

    fn wgsl_type(&self) -> Option<&'static str> {
        Some("f32")
    }

    fn view_literal(
        &self,
        data: &Self::AssociatedLiteralType,
    ) -> Element<'static, Self::Message, GraphTheme, GraphRenderer> {
        SpinSlider::new(0.0..=1.0, *data, |x| x).step(0.01).into()
    }

    fn update_literal(&self, data: &mut Self::AssociatedLiteralType, message: Self::Message) {
        *data = message;
    }

    fn literal_to_code(&self, data: &Self::AssociatedLiteralType) -> Option<String> {
        Some(format!("{:.5}", data))
    }
}

#[derive(Default, Clone)]
pub struct I32Type;

impl GraphValueType for I32Type {
    type AssociatedLiteralType = i32;

    type Message = i32;

    fn color(&self) -> Color {
        color!(0xFA2C62)
    }

    fn name(&self) -> &'static str {
        "Integer"
    }

    fn wgsl_type(&self) -> Option<&'static str> {
        Some("i32")
    }

    fn view_literal(
        &self,
        data: &Self::AssociatedLiteralType,
    ) -> Element<'static, Self::Message, GraphTheme, GraphRenderer> {
        SpinSlider::new(-10..=10, *data, |x| x).step(1).into()
    }

    fn update_literal(&self, data: &mut Self::AssociatedLiteralType, message: Self::Message) {
        *data = message;
    }

    fn literal_to_code(&self, data: &Self::AssociatedLiteralType) -> Option<String> {
        Some(format!("{}", data))
    }
}

#[derive(Default, Clone)]
pub struct U32Type;

impl GraphValueType for U32Type {
    type AssociatedLiteralType = u32;

    type Message = u32;

    fn color(&self) -> Color {
        color!(0x9555C7)
    }

    fn name(&self) -> &'static str {
        "Unsigned Integer"
    }

    fn wgsl_type(&self) -> Option<&'static str> {
        Some("u32")
    }

    fn view_literal(
        &self,
        data: &Self::AssociatedLiteralType,
    ) -> Element<'static, Self::Message, GraphTheme, GraphRenderer> {
        SpinSlider::new(0..=10, *data, |x| x).step(1u32).into()
    }

    fn update_literal(&self, data: &mut Self::AssociatedLiteralType, message: Self::Message) {
        *data = message;
    }

    fn literal_to_code(&self, data: &Self::AssociatedLiteralType) -> Option<String> {
        Some(format!("{}", data))
    }
}

#[derive(Default, Clone)]
pub struct Vec2FType;

#[derive(Clone)]
pub enum Vec2FMessage {
    X(f32),
    Y(f32),
}

impl GraphValueType for Vec2FType {
    type AssociatedLiteralType = Vec2;

    type Message = Vec2FMessage;

    fn color(&self) -> Color {
        color!(0x92E315)
    }

    fn name(&self) -> &'static str {
        "Vector2"
    }

    fn wgsl_type(&self) -> Option<&'static str> {
        Some("vec2f")
    }

    fn view_literal(
        &self,
        data: &Self::AssociatedLiteralType,
    ) -> Element<'static, Self::Message, GraphTheme, GraphRenderer> {
        column![
            SpinSlider::new(0.0..=1.0, data.x, |x| Vec2FMessage::X(x)).step(0.01),
            SpinSlider::new(0.0..=1.0, data.y, |x| Vec2FMessage::Y(x)).step(0.01),
        ]
        .padding(2)
        .into()
    }

    fn update_literal(&self, data: &mut Self::AssociatedLiteralType, message: Self::Message) {
        match message {
            Vec2FMessage::X(x) => data.x = x,
            Vec2FMessage::Y(y) => data.y = y,
        }
    }

    fn literal_to_code(&self, data: &Self::AssociatedLiteralType) -> Option<String> {
        Some(format!("vec2f({:.5}, {:.5})", data.x, data.y))
    }
}
