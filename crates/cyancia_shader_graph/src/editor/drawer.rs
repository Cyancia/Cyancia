use cyancia_widgets::{drag_drop_column::DragDropColumn, drag_field::DragField};
use iced_core::{Color, Element, Length, Point, Shadow, Theme, Vector};
use iced_widget::{Column, Text, column, container, text};
use std::{borrow::Borrow, rc::Rc};

use crate::ErasedGraphNodeCreator;

#[derive(Debug, Clone)]
pub enum NodeDrawerMessage {
    NodeCreate(usize, Point),
}

pub fn node_drawer<'a, Renderer>(
    creators: impl Iterator<Item = impl Borrow<Box<dyn ErasedGraphNodeCreator>>> + 'a,
) -> Element<'a, NodeDrawerMessage, Theme, Renderer>
where
    Renderer: iced_core::Renderer + iced_core::text::Renderer + 'a,
{
    container(
        DragDropColumn::with_children(creators.map(|c| {
            Text::new(c.borrow().create().name().to_string())
                .width(Length::Fill)
                .into()
        }))
        .width(200)
        .height(Length::Fill)
        .on_drop(|ctx| {
            if !ctx.column_bounds.contains(ctx.absolute_position) {
                let size = ctx.column_bounds.size();
                Some(NodeDrawerMessage::NodeCreate(
                    ctx.item_index,
                    ctx.absolute_position - Vector::new(size.width, 0.0),
                ))
            } else {
                None
            }
        }),
    )
    .style(|t: &Theme| container::Style {
        background: Some(t.extended_palette().background.base.color.into()),
        shadow: Shadow {
            color: Color::BLACK,
            offset: Vector::new(2.0, 0.0),
            blur_radius: 10.0,
            ..Default::default()
        },
        ..Default::default()
    })
    .into()
}
