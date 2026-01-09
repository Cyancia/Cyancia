use cyancia_id::Id;
use cyancia_widgets::circle::Circle;
use iced_core::{Color, Element, alignment::Vertical};
use iced_widget::{row, text};

use crate::{
    ErasedGraphLiteralUpdateMessage, GraphInputSlotData, GraphOutputSlotData, GraphRenderer,
    GraphTheme,
    editor::{GraphSlotId, slot_pin::SlotPin},
};

pub enum SlotSide {
    Left,
    Right,
}

pub fn input_slot(
    slot_id: Id<GraphInputSlotData>,
    slot: &GraphInputSlotData,
) -> Element<'static, ErasedGraphLiteralUpdateMessage, GraphTheme, GraphRenderer> {
    match slot.connected {
        Some(_) => empty_slot(
            slot_id.into(),
            slot.data.ty().color(),
            slot.name,
            SlotSide::Left,
        ),
        None => valued_slot(
            slot_id.into(),
            slot.data.ty().color(),
            slot.name,
            SlotSide::Left,
            slot.data.ty().view_literal(slot_id, &slot.data.value),
        ),
    }
}

pub fn output_slot<'a, Message>(
    slot_id: Id<GraphOutputSlotData>,
    slot: &GraphOutputSlotData,
) -> Element<'a, Message, GraphTheme, GraphRenderer>
where
    Message: 'a,
{
    empty_slot(
        slot_id.into(),
        slot.data.ty().color(),
        slot.name,
        SlotSide::Right,
    )
}

pub fn empty_slot<'a, Message, Theme, Renderer>(
    id: GraphSlotId,
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
    let pin = Element::new(SlotPin {
        id,
        color,
        radius: 3.0,
    });

    match slot_side {
        SlotSide::Left => row![pin, text].align_y(Vertical::Center).spacing(4).into(),
        SlotSide::Right => row![text, pin].align_y(Vertical::Center).spacing(4).into(),
    }
}

pub fn valued_slot_unconnectable<'a, Message, Theme, Renderer>(
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

    match slot_side {
        SlotSide::Left => row![text, widget]
            .align_y(Vertical::Center)
            .spacing(4)
            .into(),
        SlotSide::Right => row![widget, text]
            .align_y(Vertical::Center)
            .spacing(4)
            .into(),
    }
}

pub fn valued_slot<'a, Message, Theme, Renderer>(
    id: GraphSlotId,
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
    let pin = Element::new(SlotPin {
        id,
        color,
        radius: 3.0,
    });

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
