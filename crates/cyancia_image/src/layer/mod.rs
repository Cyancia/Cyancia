use std::{
    any::{Any, TypeId},
    collections::HashMap,
    fs::File,
    io::BufReader,
    path::Path,
};

use anyhow::Result;
use cyancia_utils::wrapper;
use dyn_clone::DynClone;
use image::DynamicImage;
use indexmap::IndexSet;
use moxcms::ColorProfile;
use parse_display::Display;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use wgpu::{Buffer, ComputePass, Device, Queue, TextureView};

use crate::{
    CImage,
    blend_modes::BlendMode,
    composite::{
        BlendFunction, BlendFunctionId, BlendFunctionRegistry, ImageCompositor,
        LayerPreviewOverriders,
    },
    layer::{group_layer::GroupLayer, pixel_layer::PixelLayer},
    tile::GpuTileStorage,
};

pub mod group_layer;
pub mod pixel_layer;

#[derive(Debug, Default)]
pub struct LayerNameGenerator {
    counters: HashMap<String, usize>,
}

impl LayerNameGenerator {
    pub fn next_of(&mut self, base: String) -> String {
        let count = self.counters.entry(base.clone()).or_insert(0);
        *count += 1;
        format!("{} {}", base, count)
    }
}

wrapper! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Display, Serialize, Deserialize, JsonSchema)]
    pub LayerId : Uuid
}

// TODO This should not be a fixed struct.
//      For example, for vector layers, the locked_channels is not valid.
//      In the future, this might be a dynamic map provided by the specific layer type?
#[derive(Clone)]
pub struct LayerData {
    id: LayerId,
    pub name: String,
    pub blend_func: BlendFunctionId,
    pub opacity: f32,
    pub is_visible: bool,
    pub is_locked: bool,
    disabled_channels: u32,
    locked_channels: u32,
    data: Box<dyn Layer>,
}

impl std::fmt::Debug for LayerData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Layer")
            .field("id", &self.id)
            .field("name", &self.name)
            .field("blend_func", &self.blend_func)
            .field("opacity", &self.opacity)
            .field("visible", &self.is_visible)
            .field("locked", &self.is_locked)
            .field("disabled_channels", &self.disabled_channels)
            .field("locked_channels", &self.locked_channels)
            .finish()
    }
}

impl LayerData {
    pub fn new(name: String, blend_func: BlendFunctionId, data: Box<dyn Layer>) -> Self {
        Self {
            id: LayerId::new(Uuid::new_v4()),
            name,
            blend_func,
            opacity: 1.0,
            is_visible: true,
            is_locked: false,
            disabled_channels: 0,
            locked_channels: 0,
            data,
        }
    }

    pub fn new_normal_pixel(name: String) -> Self {
        Self::new(name, BlendMode::Normal.id(), Box::new(PixelLayer))
    }

    pub fn new_normal_group(name: String) -> Self {
        Self::new(name, BlendMode::Normal.id(), Box::new(GroupLayer))
    }

    pub fn id(&self) -> &LayerId {
        &self.id
    }

    pub fn ty(&self) -> &dyn Layer {
        self.data.as_ref()
    }

    pub fn from_path(
        path: impl AsRef<Path>,
        tiles: &GpuTileStorage,
        dst_profile: &ColorProfile,
    ) -> Result<Self> {
        let path = path.as_ref();
        let filename = path
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        let (image, profile) = CImage::load_image_with_profile(BufReader::new(File::open(path)?))?;

        let layer = Self::from_image(filename, image, tiles, BlendMode::Normal.id());
        let layer_storage = tiles.get_layer(layer.id).unwrap();
        layer_storage.convert_color_space(&profile, dst_profile, Default::default())?;

        Ok(layer)
    }

    pub fn from_image(
        name: String,
        img: DynamicImage,
        tiles: &GpuTileStorage,
        blend_func: BlendFunctionId,
    ) -> Self {
        let layer = Self::new(name, blend_func, Box::new(PixelLayer));
        tiles.upload_image(layer.id, img);
        layer
    }

    pub fn set_channel_disabled(&mut self, channel: u32, locked: bool) {
        if locked {
            self.disabled_channels |= 1 << channel;
        } else {
            self.disabled_channels &= !(1 << channel);
        }
    }

    pub fn is_channel_disabled(&self, channel: u32) -> bool {
        self.disabled_channels & (1 << channel) != 0
    }

    pub fn set_channel_locked(&mut self, channel: u32, locked: bool) {
        if locked {
            self.locked_channels |= 1 << channel;
        } else {
            self.locked_channels &= !(1 << channel);
        }
    }

    pub fn is_channel_locked(&self, channel: u32) -> bool {
        self.locked_channels & (1 << channel) != 0
    }

    pub fn can_have_children_of(&self, maybe_child: &Self) -> bool {
        self.data
            .can_have_children_of(maybe_child.data.as_ref().type_id())
    }

    pub fn can_contain_pixels(&self) -> bool {
        self.data.can_contain_pixels()
    }

    pub fn create_blend_cache(
        &self,
        compositor: &mut ImageCompositor,
        overriders: &mut LayerPreviewOverriders,
        image: &CImage,
        tiles: &GpuTileStorage,
        blend_funcs: &BlendFunctionRegistry,
        device: &Device,
        queue: &Queue,
    ) {
        self.data.create_blend_cache(
            compositor,
            overriders,
            image,
            self.id,
            tiles,
            blend_funcs,
            device,
            queue,
        )
    }

    pub fn prepare_blend_cache(
        &self,
        compositor: &mut ImageCompositor,
        overriders: &LayerPreviewOverriders,
        image: &CImage,
        tiles: &GpuTileStorage,
        dst_buffer: &TextureView,
        dst_tile_info: &Buffer,
        output: &TextureView,
        output_tile_info: &Buffer,
        device: &Device,
        queue: &Queue,
    ) {
        self.data.prepare_blend_cache(
            compositor,
            overriders,
            image,
            self.id,
            tiles,
            dst_buffer,
            dst_tile_info,
            output,
            output_tile_info,
            device,
            queue,
        )
    }

    pub fn dispatch_blend(
        &self,
        compositor: &ImageCompositor,
        pass: &mut ComputePass,
        image: &CImage,
        tiles: &GpuTileStorage,
    ) {
        self.data
            .dispatch_blend(compositor, pass, image, self.id, tiles)
    }
}

pub trait Layer: Send + Sync + DynClone + 'static {
    fn can_have_children_of(&self, ty: TypeId) -> bool;
    fn can_contain_pixels(&self) -> bool;

    fn create_blend_cache(
        &self,
        compositor: &mut ImageCompositor,
        overriders: &mut LayerPreviewOverriders,
        image: &CImage,
        layer_id: LayerId,
        tiles: &GpuTileStorage,
        blend_funcs: &BlendFunctionRegistry,
        device: &Device,
        queue: &Queue,
    );
    fn prepare_blend_cache(
        &self,
        compositor: &mut ImageCompositor,
        overriders: &LayerPreviewOverriders,
        image: &CImage,
        layer_id: LayerId,
        tiles: &GpuTileStorage,
        dst_buffer: &TextureView,
        dst_tile_info: &Buffer,
        output: &TextureView,
        output_tile_info: &Buffer,
        device: &Device,
        queue: &Queue,
    );
    fn dispatch_blend(
        &self,
        compositor: &ImageCompositor,
        pass: &mut ComputePass,
        image: &CImage,
        layer_id: LayerId,
        tiles: &GpuTileStorage,
    );
}
dyn_clone::clone_trait_object!(Layer);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayerPosition {
    Absolute(usize),
    Above(Option<LayerId>),
    Below(Option<LayerId>),
}

impl LayerPosition {
    pub fn above(layer_id: LayerId) -> Self {
        LayerPosition::Above(Some(layer_id))
    }

    pub fn below(layer_id: LayerId) -> Self {
        LayerPosition::Below(Some(layer_id))
    }

    pub fn foreground() -> Self {
        LayerPosition::Below(None)
    }

    pub fn background() -> Self {
        LayerPosition::Above(None)
    }

    pub fn absolute(index: usize) -> Self {
        LayerPosition::Absolute(index)
    }
}

impl From<usize> for LayerPosition {
    fn from(value: usize) -> Self {
        LayerPosition::Absolute(value)
    }
}

#[derive(Debug, Clone)]
pub struct LayerStack {
    root: LayerId,
    layers: HashMap<LayerId, LayerStackNode>,
}

impl Default for LayerStack {
    fn default() -> Self {
        Self::new()
    }
}

impl LayerStack {
    pub fn new() -> Self {
        let background = LayerData::new_normal_pixel("Background".to_string());
        Self::with_background_layer(background)
    }

    pub fn with_background_layer(background: LayerData) -> Self {
        let root = LayerData::new_normal_group("Root".to_string());
        let mut root_node = LayerStackNode::without_parent(root);
        let background_node = LayerStackNode::new(*root_node.id(), background);

        root_node.insert_foreground_child(*background_node.id());

        Self {
            root: *root_node.id(),
            layers: HashMap::from([
                (*root_node.id(), root_node),
                (*background_node.id(), background_node),
            ]),
        }
    }

    pub fn root_id(&self) -> &LayerId {
        &self.root
    }

    pub fn root_node(&self) -> &LayerStackNode {
        self.layers.get(&self.root).unwrap()
    }

    pub fn add_layer(
        &mut self,
        parent_id: LayerId,
        position: impl Into<LayerPosition>,
        mut layer: LayerStackNode,
    ) {
        let Some(parent_node) = self.get_layer_mut(&parent_id) else {
            return;
        };

        if parent_node.insert_child(position, *layer.id()).is_none() {
            return;
        }
        layer.parent = Some(parent_id);
        self.layers.insert(*layer.id(), layer);
    }

    pub fn add_layer_hierarchy(
        &mut self,
        parent_id: LayerId,
        position: impl Into<LayerPosition>,
        root: LayerId,
        mut layers: HashMap<LayerId, LayerStackNode>,
    ) {
        let Some(parent_node) = self.get_layer_mut(&parent_id) else {
            return;
        };

        let Some(root_node) = layers.get_mut(&root) else {
            return;
        };

        if parent_node.insert_child(position, root).is_none() {
            return;
        }
        root_node.parent = Some(parent_id);
        self.layers.extend(layers);
    }

    pub fn insert_isolated_layer(&mut self, mut layer: LayerStackNode) {
        layer.parent = None;
        self.layers.insert(*layer.id(), layer);
    }

    pub fn sort_by_depth_and_index(
        &self,
        layers: impl IntoIterator<Item = LayerId>,
    ) -> Option<Vec<LayerId>> {
        // The deeper the closer to the front, the closer to the front in same parent, the closer to the front.
        let mut same_parent = Vec::<HashMap<LayerId, Vec<LayerId>>>::new();

        for layer in layers {
            let parent = self.get_layer(&layer)?.parent?;
            let depth = self.depth_of(&parent).unwrap() as usize;

            if depth >= same_parent.len() {
                same_parent.resize(depth + 1, HashMap::new());
            }

            same_parent[depth].entry(parent).or_default().push(layer);
        }

        for layers in &mut same_parent {
            for (parent, layers) in layers.iter_mut() {
                layers.sort_by_cached_key(|l| {
                    self.get_layer(parent).unwrap().child_index(l).unwrap()
                });
            }
        }

        Some(
            same_parent
                .into_iter()
                .rev()
                .flat_map(HashMap::into_values)
                .flatten()
                .collect(),
        )
    }

    pub fn sort_by_visual_index(&self, layers: &mut [LayerId]) {
        layers.sort_by_cached_key(|l| self.visual_index(l).unwrap());
    }

    /// Returns the visual index of a layer.
    ///
    /// For example, the visual_index of layer E D C B A is `1 2 3 4 5`
    ///
    /// ```text
    /// - A
    ///   - B
    ///     - C
    ///     - D
    ///   - E
    /// - Background
    /// ```
    pub fn visual_index(&self, layer_id: &LayerId) -> Option<usize> {
        let mut current = self.layers.get(layer_id)?;
        let mut index = self.child_count_recursive(layer_id)?;
        while let Some(parent) = current.parent().and_then(|p| self.layers.get(p)) {
            index += parent.child_index(current.id()).unwrap();
            current = parent;
        }
        Some(index)
    }

    pub fn child_count_recursive(&self, layer_id: &LayerId) -> Option<usize> {
        let mut count = 0;
        let node = self.layers.get(layer_id)?;
        for child in &node.children {
            count += self.child_count_recursive(child)?;
        }
        Some(count)
    }

    /// Returns a list of layers without overlapping ancestors.
    pub fn reduce_ancestors(&self, layers: impl IntoIterator<Item = LayerId>) -> Vec<LayerId> {
        let set = layers.into_iter().collect::<IndexSet<_>>();
        set.iter()
            .copied()
            .filter(|l| self.ancestors(*l).all(|anc| !set.contains(&anc)))
            .collect()
    }

    pub fn sort_by_depth_asc(&self, layers: &mut [LayerId]) {
        layers.sort_by_cached_key(|l| self.depth_of(l));
    }

    pub fn sort_by_depth_desc(&self, layers: &mut [LayerId]) {
        layers.sort_by_cached_key(|l| self.depth_of(l).map(|d| -(d as i32)));
    }

    pub fn len(&self) -> usize {
        self.layers.len()
    }

    pub fn is_empty(&self) -> bool {
        self.layers.is_empty()
    }

    /// Move `layer_id` so that, after the move, it is the `new_index`-th child of
    /// `new_parent_id`. `new_index` is the *final* index (the layer is inserted
    /// at `new_index` once it has already been removed from its old position), so
    /// callers never need to compensate for the index shift that removing it
    /// causes.
    pub fn move_layer(
        &mut self,
        layer_id: LayerId,
        new_parent_id: LayerId,
        new_position: impl Into<LayerPosition>,
    ) {
        if !self.layers.contains_key(&new_parent_id) {
            return;
        }

        let Some(node) = self.layers.get(&layer_id) else {
            return;
        };

        let old_parent_id = node.parent().copied();
        let old_index = old_parent_id
            .and_then(|id| self.layers.get(&id))
            .and_then(|n| n.child_index(&layer_id));

        if new_parent_id == layer_id || self.is_ancestor(&layer_id, &new_parent_id) {
            return;
        }

        let node = self.layers.get_mut(&layer_id).unwrap();
        node.parent = Some(new_parent_id);

        if let Some(old_index) = old_index {
            let old_parent = self
                .layers
                .get_mut(old_parent_id.as_ref().unwrap())
                .unwrap();
            old_parent.remove_child_at(old_index);
        }

        let new_parent = self.layers.get_mut(&new_parent_id).unwrap();
        new_parent.insert_child(new_position, layer_id).unwrap();
    }

    /// Removes a layer and all its children recursively, returning the removed nodes.
    ///
    /// The hierarchy inside of `layer_id` is preserved.
    pub fn remove_layer_hierarchy(
        &mut self,
        layer_id: &LayerId,
    ) -> HashMap<LayerId, LayerStackNode> {
        let Some(mut node) = self.layers.remove(layer_id) else {
            return HashMap::new();
        };
        if let Some(parent_node) = self.layers.get_mut(node.parent().unwrap()) {
            parent_node.remove_child(layer_id);
        }
        node.parent = None;

        fn remove_recursive(
            removed: &mut HashMap<LayerId, LayerStackNode>,
            parent_id: &LayerId,
            layer_stack: &mut LayerStack,
        ) {
            let node = layer_stack.layers.remove(parent_id).unwrap();
            for child_id in node.children.iter() {
                remove_recursive(removed, child_id, layer_stack);
            }
            removed.insert(*node.id(), node);
        }

        let mut removed = HashMap::new();
        for child_id in node.children.iter() {
            remove_recursive(&mut removed, child_id, self);
        }
        removed.insert(*node.id(), node);
        removed
    }

    pub fn get_layer_position(&self, layer_id: &LayerId) -> Option<(LayerId, usize)> {
        let parent_id = self.layers.get(layer_id)?.parent()?;
        let parent_node = self.layers.get(parent_id)?;
        Some((*parent_id, parent_node.child_index(layer_id)?))
    }

    pub fn depth_of(&self, layer_id: &LayerId) -> Option<u32> {
        let mut depth = 0;
        let mut current = self.layers.get(layer_id)?;
        while let Some(parent) = current.parent() {
            depth += 1;
            current = self.layers.get(parent)?;
        }
        Some(depth)
    }

    pub fn get_layer(&self, layer_id: &LayerId) -> Option<&LayerStackNode> {
        self.layers.get(layer_id)
    }

    pub fn get_layer_mut(&mut self, layer_id: &LayerId) -> Option<&mut LayerStackNode> {
        self.layers.get_mut(layer_id)
    }

    pub fn get_parent_of(&self, layer_id: &LayerId) -> Option<&LayerStackNode> {
        self.layers.get(self.layers.get(layer_id)?.parent()?)
    }

    pub fn get_position_of(&self, layer_id: &LayerId) -> Option<(&LayerStackNode, usize)> {
        let parent = self.get_parent_of(layer_id)?;
        let index = parent.child_index(layer_id)?;
        Some((parent, index))
    }

    pub fn iter_layers_dfs_display_order_without_root(
        &self,
    ) -> impl Iterator<Item = (&LayerStackNode, u32)> {
        let mut stack = self
            .root_node()
            .iter_children_display_order()
            // Reverse the iterator since it's a stack.
            .rev()
            .map(|child| (child, 0))
            .collect::<Vec<_>>();
        std::iter::from_fn(move || {
            let (id, depth) = stack.pop()?;
            let node = self.layers.get(id)?;
            stack.extend(
                node.iter_children_display_order()
                    .rev()
                    .map(|child| (child, depth + 1)),
            );
            Some((node, depth))
        })
    }

    pub fn iter_layers(&self) -> impl Iterator<Item = &LayerStackNode> {
        self.layers.values()
    }

    pub fn can_have_children_of(&self, parent_id: &LayerId, child_id: &LayerId) -> Option<bool> {
        let parent_layer = self.get_layer(parent_id)?;
        let child_layer = self.get_layer(child_id)?;
        Some(parent_layer.data.can_have_children_of(&child_layer.data))
    }

    /// In order from target to root, excluding the target itself.
    pub fn ancestors(&self, target: LayerId) -> impl Iterator<Item = LayerId> {
        let mut current = target;
        std::iter::from_fn(move || {
            let parent = self.layers.get(&current).and_then(|n| n.parent())?;
            current = *parent;
            Some(current)
        })
    }

    pub fn is_ancestor(&self, maybe_ancestor: &LayerId, descendant: &LayerId) -> bool {
        let mut current = descendant;
        loop {
            match self.layers.get(current).and_then(|n| n.parent()) {
                Some(parent) if parent == maybe_ancestor => return true,
                Some(parent) => current = parent,
                None => return false,
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct LayerStackNode {
    parent: Option<LayerId>,
    children: Vec<LayerId>,
    data: LayerData,
}

impl LayerStackNode {
    pub fn new(parent: LayerId, data: LayerData) -> Self {
        Self {
            parent: Some(parent),
            children: Vec::new(),
            data,
        }
    }

    pub fn without_parent(data: LayerData) -> Self {
        Self {
            parent: None,
            children: Vec::new(),
            data,
        }
    }

    pub fn id(&self) -> &LayerId {
        self.data.id()
    }

    pub fn parent(&self) -> Option<&LayerId> {
        self.parent.as_ref()
    }

    pub fn data(&self) -> &LayerData {
        &self.data
    }

    pub fn data_mut(&mut self) -> &mut LayerData {
        &mut self.data
    }

    pub fn children(&self) -> &[LayerId] {
        &self.children
    }

    pub fn children_mut(&mut self) -> &mut [LayerId] {
        &mut self.children
    }

    pub fn n_children(&self) -> usize {
        self.children.len()
    }

    pub fn swap_children(&mut self, lhs: &LayerId, rhs: &LayerId) {
        let Some(lhs_index) = self.child_index(lhs) else {
            return;
        };

        let Some(rhs_index) = self.child_index(rhs) else {
            return;
        };

        self.children.swap(lhs_index, rhs_index);
    }

    pub fn resolve_index(&self, position: LayerPosition) -> Option<usize> {
        match position {
            LayerPosition::Absolute(index) => Some(index),
            LayerPosition::Above(Some(sibling_id)) => Some(self.child_index(&sibling_id)? + 1),
            LayerPosition::Above(None) => Some(0),
            LayerPosition::Below(Some(sibling_id)) => self.child_index(&sibling_id),
            LayerPosition::Below(None) => Some(self.children.len()),
        }
    }

    pub fn iter_children_composite_order(&self) -> impl DoubleEndedIterator<Item = &LayerId> {
        self.children.iter()
    }

    pub fn iter_children_display_order(&self) -> impl DoubleEndedIterator<Item = &LayerId> {
        self.children.iter().rev()
    }

    pub fn insert_background_child(&mut self, child: LayerId) {
        self.children.insert(0, child);
    }

    pub fn insert_foreground_child(&mut self, child: LayerId) {
        self.children.push(child);
    }

    pub fn child_index(&self, child_id: &LayerId) -> Option<usize> {
        self.children.iter().position(|child| child == child_id)
    }

    pub fn insert_child(
        &mut self,
        position: impl Into<LayerPosition>,
        child: LayerId,
    ) -> Option<usize> {
        let i = self.resolve_index(position.into())?;
        self.insert_child_at(i, child);
        Some(i)
    }

    pub fn insert_child_at(&mut self, index: usize, child: LayerId) {
        self.children.insert(index, child);
    }

    pub fn child_above(&self, sibling_id: &LayerId) -> Option<LayerId> {
        self.children
            .get(self.child_index(sibling_id)? + 1)
            .cloned()
    }

    pub fn child_below(&self, sibling_id: &LayerId) -> Option<LayerId> {
        self.children
            .get(self.child_index(sibling_id)?.checked_sub(1)?)
            .cloned()
    }

    pub fn insert_child_above(&mut self, sibling_id: &LayerId, child: LayerId) {
        if let Some(index) = self.child_index(sibling_id) {
            self.children.insert(index + 1, child);
        }
    }

    pub fn insert_child_below(&mut self, sibling_id: &LayerId, child: LayerId) {
        if let Some(index) = self.child_index(sibling_id) {
            self.children.insert(index, child);
        }
    }

    pub fn remove_child(&mut self, child_id: &LayerId) {
        if let Some(index) = self.child_index(child_id) {
            self.children.remove(index);
        }
    }

    pub fn remove_child_at(&mut self, index: usize) {
        if index < self.children.len() {
            self.children.remove(index);
        }
    }
}

#[derive(Debug, Clone)]
pub struct SpecialLayers {
    selection_layer: LayerId,
}

impl SpecialLayers {
    #[allow(
        clippy::new_without_default,
        reason = "Default doesn't has the semantic of creating a new set of special layers."
    )]
    pub fn new() -> Self {
        Self {
            selection_layer: LayerId::new(Uuid::new_v4()),
        }
    }

    pub fn selection_layer(&self) -> LayerId {
        self.selection_layer
    }
}
