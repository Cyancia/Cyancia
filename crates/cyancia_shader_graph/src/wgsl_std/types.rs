use std::{collections::HashMap, sync::Arc};

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

wrapper! {
    #[derive(Default, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
    pub TextureLocalIndex : u32
}

impl TextureLocalIndex {
    pub const NULL: Self = Self(0);
}

impl GraphValueType for TextureType {
    type AssociatedLiteralType = TextureLocalIndex;

    type Message = ();

    fn color(&self) -> Color {
        color!(0x8779f2)
    }

    fn name(&self) -> &'static str {
        "Texture"
    }

    fn default_literal(&self) -> Self::AssociatedLiteralType {
        TextureLocalIndex::NULL
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
        Some(data.to_string())
    }
}
