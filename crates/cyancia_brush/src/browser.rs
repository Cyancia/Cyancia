use cyancia_assets::store::AssetStore;
use cyancia_id::Id;
use iced_core::{Element, Theme};
use iced_wgpu::Renderer;
use iced_widget::{Column, button, text};

use crate::asset::BrushPreset;

pub fn brush_asset_browser<'a, Message>(
    brushes: &AssetStore<BrushPreset>,
    on_select: impl Fn(Id<BrushPreset>) -> Message + Copy + 'a,
) -> Element<'a, Message, Theme, Renderer>
where
    Message: Clone + 'a,
{
    Column::from_iter(
        brushes
            .all()
            .iter()
            .map(move |(id, preset)| brush_asset_view(preset, *id, on_select)),
    )
    .into()
}

pub fn brush_asset_view<'a, Message>(
    preset: &BrushPreset,
    id: Id<BrushPreset>,
    on_select: impl Fn(Id<BrushPreset>) -> Message + Copy + 'a,
) -> Element<'a, Message, Theme, Renderer>
where
    Message: Clone + 'a,
{
    button(text(preset.metadata.name.clone()))
        .on_press(on_select(id))
        .into()
}
