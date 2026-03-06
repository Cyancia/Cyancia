use parse_display::Display;
use serde::{Deserialize, Serialize};

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
