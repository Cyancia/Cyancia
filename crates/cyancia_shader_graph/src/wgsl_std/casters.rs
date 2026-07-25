use glam::Vec2;

use crate::{
    graph::{slot::GraphValueType, variable::GraphVariableCaster},
    wgsl_std::types::{F32Type, Vec2FType},
};

#[derive(Default, Clone)]
pub struct F32ToVec2FCaster;

impl GraphVariableCaster for F32ToVec2FCaster {
    type FromType = F32Type;

    type ToType = Vec2FType;

    fn wgsl_cast(&self, variable: &str) -> String {
        format!("vec2f({}, {})", variable, variable)
    }

    fn cast(
        &self,
        value: &<Self::FromType as GraphValueType>::AssociatedLiteralType,
    ) -> <Self::ToType as GraphValueType>::AssociatedLiteralType {
        Vec2::splat(*value)
    }
}

#[derive(Default, Clone)]
pub struct Vec2FToF32Caster;

impl GraphVariableCaster for Vec2FToF32Caster {
    type FromType = Vec2FType;

    type ToType = F32Type;

    fn wgsl_cast(&self, variable: &str) -> String {
        format!("{}.x", variable)
    }

    fn cast(
        &self,
        value: &<Self::FromType as GraphValueType>::AssociatedLiteralType,
    ) -> <Self::ToType as GraphValueType>::AssociatedLiteralType {
        value.x
    }
}
