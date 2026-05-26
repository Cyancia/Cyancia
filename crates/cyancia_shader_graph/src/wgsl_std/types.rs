use std::{collections::HashMap, sync::Arc};

use bevy_math::Rect;
use cyancia_render::buffer::DynamicBuffer;
use cyancia_utils::wrapper;
use glam::{Vec2, Vec4};
use gpui::{Rgba, rgb};
use indexmap::IndexMap;
use parking_lot::{RwLock, RwLockReadGuard};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::graph::{
    GraphData,
    slot::{GraphInlineLiteralRenderContext, GraphValueType},
    texture::TextureId,
};

// TODO: Boolean and rectangle types

#[derive(Default, Clone)]
pub struct F32Type;

impl GraphValueType for F32Type {
    type AssociatedLiteralType = f32;

    fn color(&self) -> Rgba {
        rgb(0x0A9F8D)
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

    fn color(&self) -> Rgba {
        rgb(0x92E315)
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

    fn color(&self) -> Rgba {
        rgb(0x8779f2)
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

    fn color(&self) -> Rgba {
        rgb(0x8779f2)
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

    fn color(&self) -> Rgba {
        rgb(0x8779f2)
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

    fn literal_to_code(&self, data: &Self::AssociatedLiteralType) -> Option<String> {
        Some(format!(
            "Rect(vec2f({}, {}), vec2f({}, {}))",
            data.min.x, data.min.y, data.max.x, data.max.y
        ))
    }
}
