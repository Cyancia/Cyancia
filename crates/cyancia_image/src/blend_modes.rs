use std::sync::LazyLock;

use parse_display::Display;
use serde::{Deserialize, Serialize};

use crate::composite::{BlendFunction, BlendFunctionId};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Display)]
#[display(style = "snake_case")]
#[repr(usize)]
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

    pub const ALL_IDS: LazyLock<[BlendFunctionId; 5]> = LazyLock::new(|| {
        [
            BlendFunctionId::new("blend_normal".into()),
            BlendFunctionId::new("blend_additive".into()),
            BlendFunctionId::new("blend_subtractive".into()),
            BlendFunctionId::new("blend_multiply".into()),
            BlendFunctionId::new("blend_divide".into()),
        ]
    });

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
    fn id(&self) -> BlendFunctionId {
        Self::ALL_IDS.get(*self as usize).unwrap().clone()
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
