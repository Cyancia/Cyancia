use crate::{
    graph::variable::GraphVariableCaster,
    wgsl_std::types::{BoolType, F32Type, I32Type, Vec2FType},
};

#[derive(Default, Clone)]
pub struct F32ToVec2FCaster;

impl GraphVariableCaster for F32ToVec2FCaster {
    type FromType = F32Type;

    type ToType = Vec2FType;

    fn wgsl_cast(&self, variable: &str) -> String {
        format!("vec2f({}, {})", variable, variable)
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
}

#[derive(Default, Clone)]
pub struct F32ToI32Caster;

impl GraphVariableCaster for F32ToI32Caster {
    type FromType = F32Type;

    type ToType = I32Type;

    fn wgsl_cast(&self, variable: &str) -> String {
        format!("i32({})", variable)
    }
}

#[derive(Default, Clone)]
pub struct I32ToF32Caster;

impl GraphVariableCaster for I32ToF32Caster {
    type FromType = I32Type;

    type ToType = F32Type;

    fn wgsl_cast(&self, variable: &str) -> String {
        format!("f32({})", variable)
    }
}

#[derive(Default, Clone)]
pub struct BoolToI32Caster;

impl GraphVariableCaster for BoolToI32Caster {
    type FromType = BoolType;

    type ToType = I32Type;

    fn wgsl_cast(&self, variable: &str) -> String {
        format!("select(0i, 1i, {})", variable)
    }
}

#[derive(Default, Clone)]
pub struct I32ToBoolCaster;

impl GraphVariableCaster for I32ToBoolCaster {
    type FromType = I32Type;

    type ToType = BoolType;

    fn wgsl_cast(&self, variable: &str) -> String {
        format!("({} == 1i)", variable)
    }
}

#[derive(Default, Clone)]
pub struct I32ToVec2FCaster;

impl GraphVariableCaster for I32ToVec2FCaster {
    type FromType = I32Type;

    type ToType = Vec2FType;

    fn wgsl_cast(&self, variable: &str) -> String {
        format!("vec2f(f32({}))", variable)
    }
}

#[derive(Default, Clone)]
pub struct Vec2FToI32Caster;

impl GraphVariableCaster for Vec2FToI32Caster {
    type FromType = Vec2FType;

    type ToType = I32Type;

    fn wgsl_cast(&self, variable: &str) -> String {
        format!("i32({}.x)", variable)
    }
}
