use cyancia_widgets::circle::Circle;
use iced_core::{Color, Element, alignment::Vertical};
use iced_widget::{row, text};

pub enum SlotSide {
    Left,
    Right,
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
