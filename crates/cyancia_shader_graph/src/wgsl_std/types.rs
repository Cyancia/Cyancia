use std::{collections::HashMap, sync::Arc};

use bevy_math::Rect;
use cyancia_render::buffer::DynamicBuffer;
use cyancia_utils::wrapper;
use cyancia_widgets::spin_slider::SpinSlider;
use glam::{Vec2, Vec4};
use iced_core::{Color, Element, color};
use iced_widget::{column, space};
use indexmap::IndexMap;
use parking_lot::{RwLock, RwLockReadGuard};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{GraphRenderer, GraphTheme, graph::{slot::GraphValueType, texture::TextureId}};

// TODO: Boolean and rectangle types

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

    fn color(&self) -> Color {
        color!(0x8779f2)
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
            SpinSlider::new(0.0..=1.0, data.x, |x| ColorMessage::R(x)).step(0.01),
            SpinSlider::new(0.0..=1.0, data.y, |x| ColorMessage::G(x)).step(0.01),
            SpinSlider::new(0.0..=1.0, data.z, |x| ColorMessage::B(x)).step(0.01),
            SpinSlider::new(0.0..=1.0, data.w, |x| ColorMessage::A(x)).step(0.01),
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

    fn color(&self) -> Color {
        color!(0x8779f2)
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
        literal: &Self::AssociatedLiteralType,
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

    fn color(&self) -> Color {
        color!(0x8779f2)
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
            SpinSlider::new(0.0..=1.0, data.min.x, RectMessage::MinX).step(0.01),
            SpinSlider::new(0.0..=1.0, data.min.y, RectMessage::MinY).step(0.01),
            SpinSlider::new(0.0..=1.0, data.max.x, RectMessage::MaxX).step(0.01),
            SpinSlider::new(0.0..=1.0, data.max.y, RectMessage::MaxY).step(0.01),
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
