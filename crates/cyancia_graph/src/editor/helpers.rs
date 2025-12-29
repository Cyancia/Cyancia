use cyancia_widgets::circle::Circle;
use iced_core::{
    Color, Element, Event, Layout, Length, Point, Size, Vector,
    alignment::{Horizontal, Vertical},
    border,
    layout::{self, Limits, Node},
    mouse::Interaction,
    renderer::{Quad, Style},
    widget::{Operation, Tree},
};
use iced_graphics::geometry::Frame;
use iced_widget::{
    Renderer, Theme, column, container,
    core::{Rectangle, Widget, mouse::Cursor},
    row, text,
};

use crate::{
    ErasedSlotValue, Graph, GraphInputSlot, GraphNodeData, GraphOutputSlot, GraphSlotValueType,
    GraphSlots, InputSlotId, OutputSlotId, editor::GraphSlotId,
};

pub enum SlotSide {
    Left,
    Right,
}

impl From<GraphSlotId> for SlotSide {
    fn from(value: GraphSlotId) -> Self {
        match value {
            GraphSlotId::Input(_) => SlotSide::Left,
            GraphSlotId::Output(_) => SlotSide::Right,
        }
    }
}

pub fn empty_slot<'a, Message, Theme, Renderer>(
    color: Color,
    name: &'a str,
    slot_side: SlotSide,
) -> Element<'a, Message, Theme, Renderer>
where
    Message: 'a,
    Theme: text::Catalog + 'a,
    Renderer: iced_core::renderer::Renderer + iced_core::text::Renderer + 'a,
{
    let text = text(name);
    let pin = Element::new(Circle { color, radius: 3.0 });

    match slot_side {
        SlotSide::Left => row![pin, text].align_y(Vertical::Center).spacing(4).into(),
        SlotSide::Right => row![text, pin].align_y(Vertical::Center).spacing(4).into(),
    }
}

pub fn valued_slot<'a, Message, Theme, Renderer>(
    color: Color,
    name: &'a str,
    slot_side: SlotSide,
    widget: Element<'a, Message, Theme, Renderer>,
) -> Element<'a, Message, Theme, Renderer>
where
    Message: 'a,
    Theme: text::Catalog + 'a,
    Renderer: iced_core::renderer::Renderer + iced_core::text::Renderer + 'a,
{
    let text = text(name);
    let pin = Element::new(Circle { color, radius: 3.0 });

    match slot_side {
        SlotSide::Left => row![pin, text, widget]
            .align_y(Vertical::Center)
            .spacing(4)
            .into(),
        SlotSide::Right => row![widget, text, pin]
            .align_y(Vertical::Center)
            .spacing(4)
            .into(),
    }
}
