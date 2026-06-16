use std::{
    any::{Any, TypeId},
    collections::HashMap,
};

use cyancia_utils::wrapper;
use dyn_clone::DynClone;
use image::DynamicImage;
use parse_display::Display;
use uuid::Uuid;
use wgpu::{Buffer, ComputePass, Device, Queue, TextureView};

use crate::{
    CImage,
    blend_modes::BlendMode,
    composite::{BlendFunction, ImageCompositor, LayerPreviewOverriders},
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
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Display)]
    pub LayerId : Uuid
}

#[derive(Clone)]
pub struct LayerData {
    id: LayerId,
    pub name: String,
    pub blend_func: Box<dyn BlendFunction>,
    data: Box<dyn Layer>,
}

impl std::fmt::Debug for LayerData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Layer")
            .field("id", &self.id)
            .field("name", &self.name)
            .field("blend_func", &self.blend_func.name())
            .finish()
    }
}

impl LayerData {
    pub fn new(name: String, blend_func: Box<dyn BlendFunction>, data: Box<dyn Layer>) -> Self {
        Self {
            id: LayerId::new(Uuid::new_v4()),
            name,
            blend_func,
            data,
        }
    }

    pub fn new_normal_pixel(name: String) -> Self {
        Self::new(name, Box::new(BlendMode::Normal), Box::new(PixelLayer))
    }

    pub fn new_normal_group(name: String) -> Self {
        Self::new(name, Box::new(BlendMode::Normal), Box::new(GroupLayer))
    }

    pub fn id(&self) -> LayerId {
        self.id
    }

    pub fn from_image(
        name: String,
        img: DynamicImage,
        tiles: &GpuTileStorage,
        blend_func: Box<dyn BlendFunction>,
    ) -> Self {
        let id = LayerId::new(Uuid::new_v4());
        tiles.upload_image(id, img);

        Self {
            id,
            name,
            blend_func,
            data: Box::new(PixelLayer),
        }
    }

    pub fn can_have_children_of<T: Layer>(&self) -> bool {
        self.data.can_have_children_of(std::any::TypeId::of::<T>())
    }

    pub fn can_contain_pixels(&self) -> bool {
        self.data.can_contain_pixels()
    }

    pub fn create_blend_cache(
        &self,
        compositor: &mut ImageCompositor,
        overriders: &mut LayerPreviewOverriders,
        image: &CImage,
        node: &LayerStackNode,
        tiles: &GpuTileStorage,
        device: &Device,
        queue: &Queue,
    ) {
        self.data.create_blend_cache(
            compositor, overriders, image, self, node, tiles, device, queue,
        )
    }

    pub fn prepare_blend_cache(
        &self,
        compositor: &mut ImageCompositor,
        overriders: &LayerPreviewOverriders,
        image: &CImage,
        node: &LayerStackNode,
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
            self,
            node,
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
        node: &LayerStackNode,
        tiles: &GpuTileStorage,
    ) {
        self.data
            .dispatch_blend(compositor, pass, image, self, node, tiles)
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
        layer: &LayerData,
        node: &LayerStackNode,
        tiles: &GpuTileStorage,
        device: &Device,
        queue: &Queue,
    );
    fn prepare_blend_cache(
        &self,
        compositor: &mut ImageCompositor,
        overriders: &LayerPreviewOverriders,
        image: &CImage,
        layer: &LayerData,
        node: &LayerStackNode,
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
        layer: &LayerData,
        node: &LayerStackNode,
        tiles: &GpuTileStorage,
    );
}
dyn_clone::clone_trait_object!(Layer);

#[derive(Debug, Clone)]
pub struct LayerStack {
    root: LayerStackNode,
    layers: HashMap<LayerId, LayerData>,
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
        let mut root_node = LayerStackNode::new(root.id);
        let background_node = LayerStackNode::new(background.id);
        root_node.insert_foreground_child(background_node);

        Self {
            root: root_node,
            layers: HashMap::from([(root.id, root), (background.id, background)]),
        }
    }

    pub fn root_id(&self) -> LayerId {
        self.root.id
    }

    pub fn root_node(&self) -> &LayerStackNode {
        &self.root
    }

    pub fn add_layer(&mut self, parent_id: LayerId, layer: LayerData) {
        let parent_node = self.find_node_mut(parent_id);
        if let Some(parent_node) = parent_node {
            parent_node.insert_foreground_child(LayerStackNode::new(layer.id));
            self.layers.insert(layer.id, layer);
        }
    }

    pub fn insert_isolated_layer(&mut self, layer: LayerData) {
        self.layers.insert(layer.id, layer);
    }

    pub fn len(&self) -> usize {
        self.layers.len()
    }

    pub fn is_empty(&self) -> bool {
        self.layers.is_empty()
    }

    pub fn remove_layer(&mut self, layer_id: LayerId) -> Option<(LayerData, LayerStackNode)> {
        let node = self.find_node(layer_id)?;
        let parent = self.find_node_mut(node.parent()?)?;
        let removed_node = parent.remove_child(layer_id)?;

        let layer_data = self.layers.remove(&layer_id)?;

        Some((layer_data, removed_node))
    }

    pub fn find_node(&self, layer_id: LayerId) -> Option<&LayerStackNode> {
        find_node_recursive(&self.root, layer_id)
    }

    pub fn find_node_mut(&mut self, layer_id: LayerId) -> Option<&mut LayerStackNode> {
        find_node_mut_recursive(&mut self.root, layer_id)
    }

    pub fn get_layer(&self, layer_id: LayerId) -> Option<&LayerData> {
        self.layers.get(&layer_id)
    }

    pub fn get_layer_mut(&mut self, layer_id: LayerId) -> Option<&mut LayerData> {
        self.layers.get_mut(&layer_id)
    }

    pub fn iter_layers_dfs_display_order_without_root(
        &self,
    ) -> impl Iterator<Item = (&LayerData, u32)> {
        let mut stack = self
            .root_node()
            .iter_children_display_order()
            // Reverse the iterator since it's a stack.
            .rev()
            .map(|child| (child, 0))
            .collect::<Vec<_>>();
        std::iter::from_fn(move || {
            let (node, depth) = stack.pop()?;
            stack.extend(
                node.iter_children_display_order()
                    .rev()
                    .map(|child| (child, depth + 1)),
            );
            Some((self.layers.get(&node.id())?, depth))
        })
    }

    pub fn iter_layers(&self) -> impl Iterator<Item = &LayerData> {
        self.layers.values()
    }

    pub fn can_have_children_of(&self, parent_id: LayerId, child_id: LayerId) -> Option<bool> {
        let parent_layer = self.get_layer(parent_id)?;
        let child_layer = self.get_layer(child_id)?;
        Some(
            parent_layer
                .data
                .can_have_children_of(child_layer.data.as_ref().type_id()),
        )
    }
}

fn find_node_recursive(node: &LayerStackNode, layer_id: LayerId) -> Option<&LayerStackNode> {
    if node.id() == layer_id {
        return Some(node);
    }

    for child in node.children() {
        if let Some(found) = find_node_recursive(child, layer_id) {
            return Some(found);
        }
    }

    None
}

fn find_node_mut_recursive(
    node: &mut LayerStackNode,
    layer_id: LayerId,
) -> Option<&mut LayerStackNode> {
    if node.id() == layer_id {
        return Some(node);
    }

    for child in node.children_mut() {
        if let Some(found) = find_node_mut_recursive(child, layer_id) {
            return Some(found);
        }
    }

    None
}

#[derive(Debug, Clone)]
pub struct LayerStackNode {
    id: LayerId,
    parent: Option<LayerId>,
    children: Vec<LayerStackNode>,
}

impl LayerStackNode {
    pub fn new(id: LayerId) -> Self {
        Self {
            id,
            parent: None,
            children: Vec::new(),
        }
    }

    pub fn id(&self) -> LayerId {
        self.id
    }

    pub fn parent(&self) -> Option<LayerId> {
        self.parent
    }

    pub fn children(&self) -> &[LayerStackNode] {
        &self.children
    }

    pub fn children_mut(&mut self) -> &mut [LayerStackNode] {
        &mut self.children
    }

    pub fn n_children(&self) -> usize {
        self.children.len()
    }

    pub fn swap_children(&mut self, lhs: LayerId, rhs: LayerId) {
        let Some(lhs_index) = self.children.iter().position(|child| child.id() == lhs) else {
            return;
        };

        let Some(rhs_index) = self.children.iter().position(|child| child.id() == rhs) else {
            return;
        };

        self.children.swap(lhs_index, rhs_index);
    }

    pub fn iter_children_composite_order(
        &self,
    ) -> impl DoubleEndedIterator<Item = &LayerStackNode> {
        self.children.iter()
    }

    pub fn iter_children_composite_order_mut(
        &mut self,
    ) -> impl DoubleEndedIterator<Item = &mut LayerStackNode> {
        self.children.iter_mut()
    }

    pub fn iter_children_display_order(&self) -> impl DoubleEndedIterator<Item = &LayerStackNode> {
        self.children.iter().rev()
    }

    pub fn iter_children_display_order_mut(
        &mut self,
    ) -> impl DoubleEndedIterator<Item = &mut LayerStackNode> {
        self.children.iter_mut().rev()
    }

    pub fn insert_background_child(&mut self, mut child: LayerStackNode) {
        child.parent = Some(self.id);
        self.children.insert(0, child);
    }

    pub fn insert_foreground_child(&mut self, mut child: LayerStackNode) {
        child.parent = Some(self.id);
        self.children.push(child);
    }

    pub fn insert_child(&mut self, index: usize, mut child: LayerStackNode) {
        child.parent = Some(self.id);
        self.children.insert(index, child);
    }

    pub fn child_above(&self, sibling_id: LayerId) -> Option<&LayerStackNode> {
        let index = self
            .children
            .iter()
            .position(|child| child.id() == sibling_id)?;
        if index + 1 < self.children.len() {
            Some(&self.children[index + 1])
        } else {
            None
        }
    }

    pub fn child_above_mut(&mut self, sibling_id: LayerId) -> Option<&mut LayerStackNode> {
        let index = self
            .children
            .iter()
            .position(|child| child.id() == sibling_id)?;
        if index + 1 < self.children.len() {
            Some(&mut self.children[index + 1])
        } else {
            None
        }
    }

    pub fn child_below(&self, sibling_id: LayerId) -> Option<&LayerStackNode> {
        let index = self
            .children
            .iter()
            .position(|child| child.id() == sibling_id)?;
        if index >= 1 {
            Some(&self.children[index - 1])
        } else {
            None
        }
    }

    pub fn child_below_mut(&mut self, sibling_id: LayerId) -> Option<&mut LayerStackNode> {
        let index = self
            .children
            .iter()
            .position(|child| child.id() == sibling_id)?;
        if index >= 1 {
            Some(&mut self.children[index - 1])
        } else {
            None
        }
    }

    pub fn insert_child_above(
        &mut self,
        sibling_id: LayerId,
        mut child: LayerStackNode,
    ) -> Option<LayerStackNode> {
        if let Some(index) = self
            .children
            .iter()
            .position(|child| child.id() == sibling_id)
        {
            child.parent = Some(self.id);
            self.children.insert(index + 1, child);
            None
        } else {
            Some(child)
        }
    }

    pub fn insert_child_below(
        &mut self,
        sibling_id: LayerId,
        mut child: LayerStackNode,
    ) -> Option<LayerStackNode> {
        if let Some(index) = self
            .children
            .iter()
            .position(|child| child.id() == sibling_id)
        {
            child.parent = Some(self.id);
            self.children.insert(index, child);
            None
        } else {
            Some(child)
        }
    }

    pub fn remove_child(&mut self, child_id: LayerId) -> Option<LayerStackNode> {
        let index = self
            .children
            .iter()
            .position(|child| child.id() == child_id)?;
        self.remove_child_at(index)
    }

    pub fn remove_child_at(&mut self, index: usize) -> Option<LayerStackNode> {
        if index < self.children.len() {
            let mut child = self.children.remove(index);
            child.parent = None;
            Some(child)
        } else {
            None
        }
    }
}
