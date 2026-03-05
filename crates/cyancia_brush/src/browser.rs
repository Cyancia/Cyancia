use std::sync::Arc;

use cyancia_assets::asset::{AssetHandle, AssetId};
use cyancia_shader_graph::graph::{
    node::external::{ExternalDataStorage, ExternalLiteralId},
    slot::{ErasedGraphLiteralUpdateMessage, GraphInputSlotId, GraphLiteralUpdateMessage},
    variable::GraphValueTypeStorage,
};
use iced_core::{Element, Length, Theme};
use iced_wgpu::Renderer;
use iced_widget::{Column, button, column, pick_list, row, space, text, text_input};
use uuid::Uuid;

use crate::asset::BrushPreset;

pub fn brush_asset_browser<'a, Message>(
    brushes: impl IntoIterator<Item = (AssetId<BrushPreset>, Arc<BrushPreset>)>,
    on_select: impl Fn(AssetId<BrushPreset>) -> Message + Copy + 'a,
) -> Element<'a, Message, Theme, Renderer>
where
    Message: Clone + 'a,
{
    Column::from_iter(
        brushes
            .into_iter()
            .map(move |(id, preset)| brush_asset_view(&preset, id, on_select)),
    )
    .into()
}

pub fn brush_asset_view<'a, Message>(
    preset: &BrushPreset,
    id: AssetId<BrushPreset>,
    on_select: impl Fn(AssetId<BrushPreset>) -> Message + Copy + 'a,
) -> Element<'a, Message, Theme, Renderer>
where
    Message: Clone + 'a,
{
    button(text(preset.metadata.name.clone()))
        .on_press(on_select(id))
        .into()
}

#[derive(Clone)]
pub enum ExternalVarViewMessage {
    LiteralChanged(ExternalLiteralId, ErasedGraphLiteralUpdateMessage),
    CreateNewNameChanged(String),
    CreateNewSelectedType(&'static str),
    RequestCreateNew,
}

pub fn external_var_view<'a>(
    storage: &ExternalDataStorage,
    types: &GraphValueTypeStorage,

    create_new_name: String,
    create_new_type: Option<&'static str>,
) -> Element<'a, ExternalVarViewMessage, Theme, Renderer> {
    let all = storage.all().clone();

    let existing_vars = if all.is_empty() {
        Element::new(text("No external variables"))
    } else {
        let elems = all
            .into_iter()
            .map(|(id, var)| {
                var.ty()
                    .view_literal(GraphInputSlotId::new(Uuid::nil()), &var.value())
                    .map(move |m| ExternalVarViewMessage::LiteralChanged(id.clone(), m))
            })
            .collect::<Vec<_>>();

        Element::new(Column::from_iter(elems))
    };

    let create_new = column![
        text("Create new"),
        text_input("Name", &create_new_name).on_input(ExternalVarViewMessage::CreateNewNameChanged),
        row![
            pick_list(
                types.all().keys().copied().collect::<Vec<_>>(),
                create_new_type,
                ExternalVarViewMessage::CreateNewSelectedType,
            ),
            space().width(Length::Fill),
            button("+").on_press(ExternalVarViewMessage::RequestCreateNew)
        ],
    ]
    .spacing(2);

    column![existing_vars, space().height(Length::Fill), create_new]
        .width(150)
        .into()
}
