use std::hash::{BuildHasher, DefaultHasher, Hash, Hasher, RandomState};

use parse_display::Display;
use serde::{Deserialize, Serialize};

use crate::composite::BlendFunction;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Display)]
#[display(style = "snake_case")]
pub enum BlendMode {
    Normal,
    Additive,
    Subtractive,
    Multiply,
    Divide,
}

impl BlendMode {
    pub const ALL: [BlendMode; 5] = [
        BlendMode::Normal,
        BlendMode::Additive,
        BlendMode::Subtractive,
        BlendMode::Multiply,
        BlendMode::Divide,
    ];

    pub fn shader_func(&self) -> &'static str {
        match self {
            BlendMode::Normal => "blend_normal",
            BlendMode::Additive => "blend_additive",
            BlendMode::Subtractive => "blend_subtractive",
            BlendMode::Multiply => "blend_multiply",
            BlendMode::Divide => "blend_divide",
        }
    }
}

impl BlendFunction for BlendMode {
    fn name(&self) -> String {
        self.to_string()
    }

    fn wgsl_function_call(&self, src_ident: &str, dst_ident: &str) -> String {
        format!(
            // FIXME: This isn't working if image module is added as a whole.
            "return package::image::blend_modes::{}({}, {});",
            self.shader_func(),
            src_ident,
            dst_ident
        )
    }
}
