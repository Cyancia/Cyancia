use std::rc::Rc;

use bevy_math::Rect;
use cyancia_render::buffer::DynamicBuffer;
use cyancia_utils::random_oklch;
use glam::{Vec2, Vec4};
use gpui::{
    AnyElement, App, AppContext, Context, ElementId, Entity, InteractiveElement, IntoElement,
    ParentElement, Pixels, Rgba, Styled, Window, div, px,
};
use gpui_component::{
    Sizable,
    input::{InputEvent, InputState, MaskPattern, NumberInput, NumberInputEvent, StepAction},
};
use serde::{Deserialize, Serialize};

use crate::graph::{
    slot::{GraphInlineLiteralRenderContext, GraphValueType},
    texture::TextureId,
    variable::GraphLiteralValue,
};

pub const MIN_INLINE_FLOAT_WIDTH: Pixels = px(120.0);

#[derive(Default, Clone)]
pub struct F32Type;

impl GraphValueType for F32Type {
    type AssociatedLiteralType = f32;

    fn color(&self, cx: &App) -> Rgba {
        random_oklch!(F32Type, cx)
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

    fn render_inline(
        &self,
        literal: &Self::AssociatedLiteralType,
        mut ctx: GraphInlineLiteralRenderContext<'_>,
    ) -> AnyElement {
        let literal = *literal;
        literal_number_input(
            format!("slot-literal-{}", ctx.slot_id),
            &mut ctx,
            literal,
            std::convert::identity,
        )
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

    fn color(&self, cx: &App) -> Rgba {
        random_oklch!(Vec2FType, cx)
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

    fn render_inline(
        &self,
        literal: &Self::AssociatedLiteralType,
        mut ctx: GraphInlineLiteralRenderContext<'_>,
    ) -> AnyElement {
        let literal = *literal;
        let x = literal_number_input(
            format!("slot-literal-x-{}", ctx.slot_id),
            &mut ctx,
            literal.x,
            move |val| literal.with_x(val),
        );
        let y = literal_number_input(
            format!("slot-literal-y-{}", ctx.slot_id),
            &mut ctx,
            literal.y,
            move |val| literal.with_y(val),
        );

        div().w_full().child(x).child(y).into_any_element()
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

    fn color(&self, cx: &App) -> Rgba {
        random_oklch!(ColorType, cx)
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

    fn render_inline(
        &self,
        literal: &Self::AssociatedLiteralType,
        mut ctx: GraphInlineLiteralRenderContext<'_>,
    ) -> AnyElement {
        let literal = *literal;
        let r = literal_number_input(
            format!("slot-literal-r-{}", ctx.slot_id),
            &mut ctx,
            literal.x,
            move |val| literal.with_x(val),
        );
        let g = literal_number_input(
            format!("slot-literal-g-{}", ctx.slot_id),
            &mut ctx,
            literal.y,
            move |val| literal.with_y(val),
        );
        let b = literal_number_input(
            format!("slot-literal-b-{}", ctx.slot_id),
            &mut ctx,
            literal.z,
            move |val| literal.with_z(val),
        );
        let a = literal_number_input(
            format!("slot-literal-a-{}", ctx.slot_id),
            &mut ctx,
            literal.w,
            move |val| literal.with_w(val),
        );

        div()
            .w_full()
            .child(r)
            .child(g)
            .child(b)
            .child(a)
            .into_any_element()
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

    fn color(&self, cx: &App) -> Rgba {
        random_oklch!(TextureType, cx)
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

    fn try_write_into_shader_buffer(&self, _: &Self::AssociatedLiteralType) -> Option<Vec<u8>> {
        None
    }

    fn literal_to_code(&self, data: &Self::AssociatedLiteralType) -> Option<String> {
        Some(data.local_index.to_string())
    }

    fn render_inline(
        &self,
        _: &Self::AssociatedLiteralType,
        _: GraphInlineLiteralRenderContext<'_>,
    ) -> AnyElement {
        div().into_any_element()
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

    fn color(&self, cx: &App) -> Rgba {
        random_oklch!(RectType, cx)
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

    fn render_inline(
        &self,
        literal: &Self::AssociatedLiteralType,
        mut ctx: GraphInlineLiteralRenderContext<'_>,
    ) -> AnyElement {
        let literal = *literal;
        let minx = literal_number_input(
            format!("slot-literal-minx-{}", ctx.slot_id),
            &mut ctx,
            literal.min.x,
            move |val| Rect {
                min: literal.min.with_x(val),
                max: literal.max,
            },
        );
        let miny = literal_number_input(
            format!("slot-literal-miny-{}", ctx.slot_id),
            &mut ctx,
            literal.min.y,
            move |val| Rect {
                min: literal.min.with_y(val),
                max: literal.max,
            },
        );
        let maxx = literal_number_input(
            format!("slot-literal-maxx-{}", ctx.slot_id),
            &mut ctx,
            literal.max.x,
            move |val| Rect {
                min: literal.min,
                max: literal.max.with_x(val),
            },
        );
        let maxy = literal_number_input(
            format!("slot-literal-maxy-{}", ctx.slot_id),
            &mut ctx,
            literal.max.y,
            move |val| Rect {
                min: literal.min,
                max: literal.max.with_y(val),
            },
        );

        div()
            .w_full()
            .child(minx)
            .child(miny)
            .child(maxx)
            .child(maxy)
            .into_any_element()
    }
}

struct LiteralNumberInputState {
    input_state: Entity<InputState>,
    value: f32,
}

impl LiteralNumberInputState {
    fn new<T: GraphLiteralValue>(
        initial_value: f32,
        on_update: Rc<dyn Fn(Box<dyn GraphLiteralValue>, &mut App)>,
        updated_literal: impl Fn(f32) -> T + 'static,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let input_state = cx.new(|cx| {
            let mut state = InputState::new(window, cx).mask_pattern(MaskPattern::Number {
                separator: None,
                fraction: Some(4),
            });
            state.set_value(format!("{:.4}", initial_value), window, cx);
            state
        });

        cx.subscribe_in(&input_state, window, {
            let on_update = on_update.clone();
            move |state, input_state, event: &InputEvent, window, cx| match event {
                InputEvent::PressEnter { .. } | InputEvent::Blur => {
                    input_state.update(cx, |input_state, cx| {
                        if let Ok(value) = input_state.value().parse::<f32>() {
                            (on_update)(Box::new(updated_literal(value)), cx);
                            state.value = value;
                            input_state.set_value(format!("{:.4}", value), window, cx);
                        } else {
                            input_state.set_value(format!("{:.4}", state.value), window, cx);
                        }
                    });
                }
                InputEvent::Change | InputEvent::Focus => {}
            }
        })
        .detach();
        cx.subscribe_in(&input_state, window, {
            let on_update = on_update;
            move |state, input_state, event: &NumberInputEvent, window, cx| match event {
                NumberInputEvent::Step(step) => {
                    let delta = match step {
                        StepAction::Increment => 0.1,
                        StepAction::Decrement => -0.1,
                    };
                    let value = state.input_state.read(cx).value();
                    let Ok(value) = value.parse::<f32>() else {
                        return;
                    };
                    let value = value + delta;
                    state.value = value;
                    input_state.update(cx, |input_state, cx| {
                        input_state.set_value(format!("{:.4}", value), window, cx);
                    });
                    (on_update)(Box::new(value), cx);
                }
            }
        })
        .detach();

        Self {
            input_state,
            value: initial_value,
        }
    }
}

fn literal_number_input<T: GraphLiteralValue>(
    id: impl Into<ElementId>,
    ctx: &mut GraphInlineLiteralRenderContext<'_>,
    initial_value: f32,
    updated_literal: impl Fn(f32) -> T + 'static,
) -> AnyElement {
    let input_state = ctx.window.use_keyed_state(id, ctx.cx, |window, cx| {
        LiteralNumberInputState::new(
            initial_value,
            ctx.on_update.clone(),
            updated_literal,
            window,
            cx,
        )
    });

    let input_state = input_state.read(ctx.cx);
    div()
        .w_full()
        .min_w(MIN_INLINE_FLOAT_WIDTH)
        .child(NumberInput::new(&input_state.input_state).small())
        .on_any_mouse_down(|_, _, cx| cx.stop_propagation())
        .into_any_element()
}
