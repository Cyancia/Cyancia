use std::{any::Any, collections::HashMap};

use cyancia_id::Id;
use iced_core::{
    Border, Color, Element, Length, Point, Rectangle, Size, Widget, layout, mouse,
    renderer::{self, Quad},
    widget::{Operation, Tree, tree},
};

use crate::{GraphInputSlotData, GraphOutputSlotData, editor::GraphSlotId};

#[derive(Default)]
pub struct GraphSlotPinPositionCollection {
    slots: HashMap<GraphSlotId, Point>,
}

impl Operation for GraphSlotPinPositionCollection {
    fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation<()>)) {
        operate(self);
    }

    fn custom(&mut self, id: Option<&iced_widget::Id>, bounds: Rectangle, state: &mut dyn Any) {
        if let Some(state) = state.downcast_ref::<SlotPinState>() {
            self.slots.insert(state.id.clone(), bounds.center());
        }
    }
}

impl GraphSlotPinPositionCollection {
    pub fn get(&self, slot_id: &GraphSlotId) -> Option<&Point> {
        self.slots.get(slot_id)
    }

    pub fn get_input(&self, slot_id: &Id<GraphInputSlotData>) -> Option<&Point> {
        self.slots.get(&GraphSlotId::Input(*slot_id))
    }

    pub fn get_output(&self, slot_id: &Id<GraphOutputSlotData>) -> Option<&Point> {
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
        tree: &mut Tree,
        renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        let d = self.radius * 2.0;
        layout::atomic(limits, d, d)
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut Renderer,
        theme: &Theme,
        style: &renderer::Style,
        layout: layout::Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
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
        tree: &mut Tree,
        layout: layout::Layout<'_>,
        renderer: &Renderer,
        operation: &mut dyn Operation,
    ) {
        operation.custom(None, layout.bounds(), &mut SlotPinState { id: self.id });
    }
}

struct SlotPinState {
    id: GraphSlotId,
}
