use cyancia_assets::asset::AssetHandle;
use cyancia_shader_graph::save::SerializableGraphFunction;
use gpui::{App, Context, IntoElement, ParentElement, RenderOnce, Window};
use gpui_component::{
    IndexPath, Selectable,
    list::{ListDelegate, ListItem, ListState},
};

use crate::asset::BrushPreset;

#[derive(IntoElement)]
pub struct BrushPresetListItem {
    base: ListItem,
    pub handle: AssetHandle<BrushPreset>,
    is_selected: bool,
}

impl BrushPresetListItem {
    pub fn new(brush: AssetHandle<BrushPreset>) -> Self {
        Self {
            base: ListItem::new(*brush.clone().id()),
            handle: brush,
            is_selected: false,
        }
    }
}

impl Selectable for BrushPresetListItem {
    fn selected(mut self, selected: bool) -> Self {
        self.is_selected = selected;
        self
    }

    fn is_selected(&self) -> bool {
        self.is_selected
    }
}

impl RenderOnce for BrushPresetListItem {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        let Ok(brush) = self.handle.get() else {
            return self
                .base
                .child(format!("Unable to load brush {}", self.handle.id()));
        };
        self.base
            .selected(self.is_selected)
            .child(brush.metadata.name.clone())
    }
}

pub struct BrushPresetListDelegate {
    items: Vec<BrushPresetListItem>,
    selected_index: Option<IndexPath>,
}

impl BrushPresetListDelegate {
    pub fn new(brushes: Vec<AssetHandle<BrushPreset>>) -> Self {
        Self {
            items: brushes.into_iter().map(BrushPresetListItem::new).collect(),
            selected_index: None,
        }
    }

    pub fn get(&self, ix: IndexPath) -> Option<&BrushPresetListItem> {
        self.items.get(ix.row)
    }

    pub fn items(&self) -> &[BrushPresetListItem] {
        &self.items
    }
}

impl ListDelegate for BrushPresetListDelegate {
    type Item = BrushPresetListItem;

    fn items_count(&self, _: usize, _: &App) -> usize {
        self.items.len()
    }

    fn render_item(
        &mut self,
        ix: IndexPath,
        _: &mut Window,
        _: &mut Context<ListState<Self>>,
    ) -> Option<Self::Item> {
        let item = self.items.get(ix.row)?;
        Some(BrushPresetListItem::new(item.handle.clone()))
    }

    fn set_selected_index(
        &mut self,
        ix: Option<IndexPath>,
        _: &mut Window,
        _: &mut Context<ListState<Self>>,
    ) {
        self.selected_index = ix;
    }
}

#[derive(IntoElement)]
pub struct BrushFunctionItem {
    base: ListItem,
    pub handle: AssetHandle<SerializableGraphFunction>,
    is_selected: bool,
}

impl BrushFunctionItem {
    pub fn new(func: AssetHandle<SerializableGraphFunction>) -> Self {
        Self {
            base: ListItem::new(*func.clone().id()),
            handle: func,
            is_selected: false,
        }
    }
}

impl Selectable for BrushFunctionItem {
    fn selected(mut self, selected: bool) -> Self {
        self.is_selected = selected;
        self
    }

    fn is_selected(&self) -> bool {
        self.is_selected
    }
}

impl RenderOnce for BrushFunctionItem {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        let Ok(func) = self.handle.get() else {
            return self.base.child(format!(
                "Unable to load brush function {}",
                self.handle.id()
            ));
        };
        self.base
            .selected(self.is_selected)
            .child(func.name.clone())
    }
}

pub struct BrushFunctionListDelegate {
    items: Vec<BrushFunctionItem>,
    selected_index: Option<IndexPath>,
}

impl BrushFunctionListDelegate {
    pub fn new(funcs: Vec<AssetHandle<SerializableGraphFunction>>) -> Self {
        Self {
            items: funcs.into_iter().map(BrushFunctionItem::new).collect(),
            selected_index: None,
        }
    }

    pub fn get(&self, ix: IndexPath) -> Option<&BrushFunctionItem> {
        self.items.get(ix.row)
    }
}

impl ListDelegate for BrushFunctionListDelegate {
    type Item = BrushFunctionItem;

    fn items_count(&self, _: usize, _: &App) -> usize {
        self.items.len()
    }

    fn render_item(
        &mut self,
        ix: IndexPath,
        _: &mut Window,
        _: &mut Context<ListState<Self>>,
    ) -> Option<Self::Item> {
        let item = self.items.get(ix.row)?;
        Some(BrushFunctionItem::new(item.handle.clone()))
    }

    fn set_selected_index(
        &mut self,
        ix: Option<IndexPath>,
        _: &mut Window,
        _: &mut Context<ListState<Self>>,
    ) {
        self.selected_index = ix;
    }
}
