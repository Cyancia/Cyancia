use crate::{
    graph::variable::GraphVariableCaster,
    wgsl_std::types::{F32Type, Vec2FType},
};

#[derive(Default)]
pub struct F32ToVec2FCaster;

impl GraphVariableCaster for F32ToVec2FCaster {
    type FromType = F32Type;

    type ToType = Vec2FType;

    fn cast(&self, variable: &String) -> String {
        format!("vec2f({}, 0.0)", variable)
    }
}

#[derive(Default)]
pub struct Vec2FToF32Caster;

impl GraphVariableCaster for Vec2FToF32Caster {
    type FromType = Vec2FType;

    type ToType = F32Type;

    fn cast(&self, variable: &String) -> String {
        format!("{}.x", variable)
    }
}
