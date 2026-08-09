use std::convert::identity;

use bevy_math::Rect;
use cyancia_render::buffer::DynamicBuffer;
use cyancia_widgets::spin_slider::SpinSlider;
use glam::{Vec2, Vec4};
use iced_core::{Color, Element};
use iced_widget::{checkbox, column, space};
use serde::{Deserialize, Serialize};

use crate::{
    GraphRenderer, GraphTheme,
    graph::{slot::GraphValueType, texture::TextureId},
    wgsl_std::themed_color,
};

#[derive(Default, Clone)]
pub struct F32Type;

impl GraphValueType for F32Type {
    type AssociatedLiteralType = f32;

    type Message = f32;

    fn color(&self, is_dark: bool) -> Color {
        themed_color(stringify!(F32Type), is_dark)
    }

    fn name(&self) -> &'static str {
        "Float"
    }

    fn default_literal(&self) -> Self::AssociatedLiteralType {
        0.0
    }

    fn wgsl_type(&self) -> Option<&'static str> {
        Some("f32")
    }

    fn try_write_into_shader_buffer(
        &self,
        literal: &Self::AssociatedLiteralType,
    ) -> Option<Vec<u8>> {
        let mut buf = DynamicBuffer::default();
        buf.push(literal);
        Some(buf.into_inner())
    }

    fn view_literal(
        &self,
        data: &Self::AssociatedLiteralType,
    ) -> Element<'static, Self::Message, GraphTheme, GraphRenderer> {
        SpinSlider::new(0.0..=1.0, *data)
            .on_confirm(identity)
            .step(0.01)
            .into()
    }

    fn update_literal(&self, data: &mut Self::AssociatedLiteralType, message: Self::Message) {
        *data = message;
    }

    fn literal_to_code(&self, data: &Self::AssociatedLiteralType) -> Option<String> {
        Some(format!("{:.5}", data))
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

    fn color(&self, is_dark: bool) -> Color {
        themed_color(stringify!(Vec2FType), is_dark)
    }

    fn name(&self) -> &'static str {
        "Vector2"
    }

    fn default_literal(&self) -> Self::AssociatedLiteralType {
        Vec2::ZERO
    }

    fn wgsl_type(&self) -> Option<&'static str> {
        Some("vec2f")
    }

    fn try_write_into_shader_buffer(
        &self,
        literal: &Self::AssociatedLiteralType,
    ) -> Option<Vec<u8>> {
        let mut buf = DynamicBuffer::default();
        buf.push(literal);
        Some(buf.into_inner())
    }

    fn view_literal(
        &self,
        data: &Self::AssociatedLiteralType,
    ) -> Element<'static, Self::Message, GraphTheme, GraphRenderer> {
        column![
            SpinSlider::new(0.0..=1.0, data.x).on_confirm(Vec2FMessage::X),
            SpinSlider::new(0.0..=1.0, data.y).on_confirm(Vec2FMessage::Y),
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

#[derive(Default, Clone)]
pub struct I32Type;

impl GraphValueType for I32Type {
    type AssociatedLiteralType = i32;

    type Message = i32;

    fn color(&self, is_dark: bool) -> Color {
        themed_color(stringify!(I32Type), is_dark)
    }

    fn name(&self) -> &'static str {
        "Integer"
    }

    fn default_literal(&self) -> Self::AssociatedLiteralType {
        0
    }

    fn wgsl_type(&self) -> Option<&'static str> {
        Some("i32")
    }

    fn try_write_into_shader_buffer(
        &self,
        literal: &Self::AssociatedLiteralType,
    ) -> Option<Vec<u8>> {
        let mut buf = DynamicBuffer::default();
        buf.push(literal);
        Some(buf.into_inner())
    }

    fn literal_to_code(&self, data: &Self::AssociatedLiteralType) -> Option<String> {
        Some(format!("{data}i"))
    }

    fn view_literal(
        &self,
        data: &Self::AssociatedLiteralType,
    ) -> Element<'static, Self::Message, GraphTheme, GraphRenderer> {
        SpinSlider::new(-10..=10, *data).on_confirm(identity).into()
    }

    fn update_literal(&self, data: &mut Self::AssociatedLiteralType, message: Self::Message) {
        *data = message;
    }
}

#[derive(Default, Clone)]
pub struct BoolType;

impl GraphValueType for BoolType {
    type AssociatedLiteralType = bool;

    type Message = bool;

    fn color(&self, is_dark: bool) -> Color {
        themed_color(stringify!(BoolType), is_dark)
    }

    fn name(&self) -> &'static str {
        "Bool"
    }

    fn default_literal(&self) -> Self::AssociatedLiteralType {
        false
    }

    fn wgsl_type(&self) -> Option<&'static str> {
        Some("bool")
    }

    fn try_write_into_shader_buffer(&self, _: &Self::AssociatedLiteralType) -> Option<Vec<u8>> {
        None
    }

    fn literal_to_code(&self, data: &Self::AssociatedLiteralType) -> Option<String> {
        Some(data.to_string())
    }

    fn view_literal(
        &self,
        data: &Self::AssociatedLiteralType,
    ) -> Element<'static, Self::Message, GraphTheme, GraphRenderer> {
        checkbox(*data).on_toggle(std::convert::identity).into()
    }

    fn update_literal(&self, data: &mut Self::AssociatedLiteralType, message: Self::Message) {
        *data = message;
    }
}

#[derive(Default, Clone)]
pub struct ColorType;

#[derive(Debug, Clone)]
pub enum ColorMessage {
    R(f32),
    G(f32),
    B(f32),
    A(f32),
}

impl GraphValueType for ColorType {
    type AssociatedLiteralType = Vec4;

    type Message = ColorMessage;

    fn color(&self, is_dark: bool) -> Color {
        themed_color(stringify!(ColorType), is_dark)
    }

    fn name(&self) -> &'static str {
        "Color"
    }

    fn default_literal(&self) -> Self::AssociatedLiteralType {
        Vec4::ZERO
    }

    fn wgsl_type(&self) -> Option<&'static str> {
        Some("vec4f")
    }

    fn try_write_into_shader_buffer(
        &self,
        literal: &Self::AssociatedLiteralType,
    ) -> Option<Vec<u8>> {
        let mut buf = DynamicBuffer::default();
        buf.push(literal);
        Some(buf.into_inner())
    }

    fn view_literal(
        &self,
        data: &Self::AssociatedLiteralType,
    ) -> Element<'static, Self::Message, GraphTheme, GraphRenderer> {
        column![
            SpinSlider::new(0.0..=1.0, data.x)
                .on_confirm(ColorMessage::R)
                .allow_beyond_range(false),
            SpinSlider::new(0.0..=1.0, data.y)
                .on_confirm(ColorMessage::G)
                .allow_beyond_range(false),
            SpinSlider::new(0.0..=1.0, data.z)
                .on_confirm(ColorMessage::B)
                .allow_beyond_range(false),
            SpinSlider::new(0.0..=1.0, data.w)
                .on_confirm(ColorMessage::A)
                .allow_beyond_range(false),
        ]
        .padding(2)
        .into()
    }

    fn update_literal(&self, data: &mut Self::AssociatedLiteralType, message: Self::Message) {
        match message {
            ColorMessage::R(r) => data.x = r,
            ColorMessage::G(g) => data.y = g,
            ColorMessage::B(b) => data.z = b,
            ColorMessage::A(a) => data.w = a,
        }
    }

    fn literal_to_code(&self, data: &Self::AssociatedLiteralType) -> Option<String> {
        Some(format!(
            "vec4f({:.5}, {:.5}, {:.5}, {:.5})",
            data.x, data.y, data.z, data.w
        ))
    }
}

#[derive(Default, Clone)]
pub struct TextureType;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TextureReference {
    pub local_index: u32,
    pub external_id: TextureId,
}

impl Default for TextureReference {
    fn default() -> Self {
        Self::NULL
    }
}

impl TextureReference {
    pub const NULL: Self = Self {
        local_index: 0,
        external_id: TextureId::NULL,
    };
}

impl GraphValueType for TextureType {
    type AssociatedLiteralType = TextureReference;

    type Message = ();

    fn color(&self, is_dark: bool) -> Color {
        themed_color(stringify!(TextureType), is_dark)
    }

    fn name(&self) -> &'static str {
        "Texture"
    }

    fn default_literal(&self) -> Self::AssociatedLiteralType {
        TextureReference::NULL
    }

    fn wgsl_type(&self) -> Option<&'static str> {
        None
    }

    fn try_write_into_shader_buffer(
        &self,
        _literal: &Self::AssociatedLiteralType,
    ) -> Option<Vec<u8>> {
        None
    }

    fn view_literal(
        &self,
        _data: &Self::AssociatedLiteralType,
    ) -> Element<'static, Self::Message, GraphTheme, GraphRenderer> {
        Element::new(space())
    }

    fn update_literal(&self, _data: &mut Self::AssociatedLiteralType, _message: Self::Message) {}

    fn literal_to_code(&self, data: &Self::AssociatedLiteralType) -> Option<String> {
        Some(data.local_index.to_string())
    }
}

#[derive(Default, Clone)]
pub struct RectType;

#[derive(Debug, Clone)]
pub enum RectMessage {
    MinX(f32),
    MinY(f32),
    MaxX(f32),
    MaxY(f32),
}

impl GraphValueType for RectType {
    type AssociatedLiteralType = Rect;

    type Message = RectMessage;

    fn color(&self, is_dark: bool) -> Color {
        themed_color(stringify!(RectType), is_dark)
    }

    fn name(&self) -> &'static str {
        "Rectangle"
    }

    fn default_literal(&self) -> Self::AssociatedLiteralType {
        Rect::default()
    }

    fn wgsl_type(&self) -> Option<&'static str> {
        Some("Rect")
    }

    fn try_write_into_shader_buffer(
        &self,
        literal: &Self::AssociatedLiteralType,
    ) -> Option<Vec<u8>> {
        let mut buf = DynamicBuffer::default();
        buf.push(literal);
        Some(buf.into_inner())
    }

    fn view_literal(
        &self,
        data: &Self::AssociatedLiteralType,
    ) -> Element<'static, Self::Message, GraphTheme, GraphRenderer> {
        column![
            SpinSlider::new(0.0..=1.0, data.min.x).on_confirm(RectMessage::MinX),
            SpinSlider::new(0.0..=1.0, data.min.y).on_confirm(RectMessage::MinY),
            SpinSlider::new(0.0..=1.0, data.max.x).on_confirm(RectMessage::MaxX),
            SpinSlider::new(0.0..=1.0, data.max.y).on_confirm(RectMessage::MaxY),
        ]
        .padding(2)
        .into()
    }

    fn update_literal(&self, data: &mut Self::AssociatedLiteralType, message: Self::Message) {
        match message {
            RectMessage::MinX(x) => data.min.x = x,
            RectMessage::MinY(y) => data.min.y = y,
            RectMessage::MaxX(x) => data.max.x = x,
            RectMessage::MaxY(y) => data.max.y = y,
        }
    }

    fn literal_to_code(&self, data: &Self::AssociatedLiteralType) -> Option<String> {
        Some(format!(
            "Rect(vec2f({}, {}), vec2f({}, {}))",
            data.min.x, data.min.y, data.max.x, data.max.y
        ))
    }
}
