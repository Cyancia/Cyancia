use std::{any::Any, collections::HashMap};

use iced_core::{
    Border, Color, Element, Length, Point, Rectangle, Size, Widget,
    alignment::Vertical,
    layout, mouse,
    renderer::{self, Quad},
    text::IntoFragment,
    widget::{Operation, Tree},
};
use iced_widget::{row, text};

use crate::{
    GraphRenderer, GraphTheme,
    graph::slot::{
        ErasedGraphLiteralUpdateMessage, GraphInputSlotData, GraphInputSlotId, GraphOutputSlotData,
        GraphOutputSlotId,
    },
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GraphSlotId {
    Input(GraphInputSlotId),
    Output(GraphOutputSlotId),
}

impl From<GraphInputSlotId> for GraphSlotId {
    fn from(value: GraphInputSlotId) -> Self {
        GraphSlotId::Input(value)
    }
}

impl From<GraphOutputSlotId> for GraphSlotId {
    fn from(value: GraphOutputSlotId) -> Self {
        GraphSlotId::Output(value)
    }
}

#[derive(Default)]
pub struct GraphSlotPinPositionCollection {
    slots: HashMap<GraphSlotId, Point>,
}

impl Operation for GraphSlotPinPositionCollection {
    fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation<()>)) {
        operate(self);
    }

    fn custom(&mut self, _id: Option<&iced_widget::Id>, bounds: Rectangle, state: &mut dyn Any) {
        if let Some(state) = state.downcast_ref::<SlotPinState>() {
            self.slots.insert(state.id, bounds.center());
        }
    }
}

impl GraphSlotPinPositionCollection {
    pub fn get(&self, slot_id: &GraphSlotId) -> Option<&Point> {
        self.slots.get(slot_id)
    }

    pub fn get_input(&self, slot_id: &GraphInputSlotId) -> Option<&Point> {
        self.slots.get(&GraphSlotId::Input(*slot_id))
    }

    pub fn get_output(&self, slot_id: &GraphOutputSlotId) -> Option<&Point> {
        self.slots.get(&GraphSlotId::Output(*slot_id))
    }

    pub fn clear(&mut self) {
        self.slots.clear();
    }

    pub fn all(&self) -> impl Iterator<Item = (&GraphSlotId, &Point)> {
        self.slots.iter()
    }
}

pub struct SlotPin {
    pub id: GraphSlotId,
    pub radius: f32,
    pub color: Color,
}

impl<Message, Theme, Renderer> Widget<Message, Theme, Renderer> for SlotPin
where
    Renderer: iced_core::Renderer,
{
    fn size(&self) -> Size<Length> {
        let d = self.radius * 2.0;
        Size::new(Length::Fixed(d), Length::Fixed(d))
    }

    fn layout(
        &mut self,
        _tree: &mut Tree,
        _renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        let d = self.radius * 2.0;
        layout::atomic(limits, d, d)
    }

    fn draw(
        &self,
        _tree: &Tree,
        renderer: &mut Renderer,
        _theme: &Theme,
        _style: &renderer::Style,
        layout: layout::Layout<'_>,
        _cursor: mouse::Cursor,
        _viewport: &Rectangle,
    ) {
        renderer.fill_quad(
            Quad {
                bounds: layout.bounds(),
                border: Border::default().rounded(self.radius),
                ..Default::default()
            },
            self.color,
        );
    }

    fn operate(
        &mut self,
        _tree: &mut Tree,
        layout: layout::Layout<'_>,
        _renderer: &Renderer,
        operation: &mut dyn Operation,
    ) {
        operation.custom(None, layout.bounds(), &mut SlotPinState { id: self.id });
    }
}

struct SlotPinState {
    id: GraphSlotId,
}
pub enum SlotSide {
    Left,
    Right,
}

pub fn input_slot<'a>(
    slot_id: GraphInputSlotId,
    slot_name: impl IntoFragment<'a>,
    slot: &GraphInputSlotData,
    is_dark: bool,
) -> Element<'a, ErasedGraphLiteralUpdateMessage, GraphTheme, GraphRenderer> {
    match slot.connected {
        Some(_) => empty_slot(
            slot_id.into(),
            slot.data.ty().color(is_dark),
            slot_name,
            SlotSide::Left,
        ),
        None => valued_slot(
            slot_id.into(),
            slot.data.ty().color(is_dark),
            slot_name,
            SlotSide::Left,
            slot.data.ty().view_literal(slot_id, slot.data.value()),
        ),
    }
}

pub fn output_slot<'a, Message>(
    slot_id: GraphOutputSlotId,
    slot_name: impl IntoFragment<'a>,
    slot: &GraphOutputSlotData,
    is_dark: bool,
) -> Element<'a, Message, GraphTheme, GraphRenderer>
where
    Message: 'a,
{
    empty_slot(
        slot_id.into(),
        slot.data_ty.color(is_dark),
        slot_name,
        SlotSide::Right,
    )
}

pub fn empty_slot<'a, Message, Theme, Renderer>(
    id: GraphSlotId,
    color: Color,
    name: impl IntoFragment<'a>,
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
        SlotSide::Left => row![pin, text]
            .width(Length::Fill)
            .align_y(Vertical::Center)
            .spacing(4)
            .into(),
        SlotSide::Right => row![iced_widget::space().width(Length::Fill), text, pin]
            .width(Length::Fill)
            .align_y(Vertical::Center)
            .spacing(4)
            .into(),
    }
}

pub fn valued_slot<'a, Message, Theme, Renderer>(
    id: GraphSlotId,
    color: Color,
    name: impl IntoFragment<'a>,
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
            .width(Length::Fill)
            .align_y(Vertical::Center)
            .spacing(4)
            .into(),
        SlotSide::Right => row![iced_widget::space().width(Length::Fill), widget, text, pin]
            .width(Length::Fill)
            .align_y(Vertical::Center)
            .spacing(4)
            .into(),
    }
}
