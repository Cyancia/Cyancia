use std::{collections::HashMap, sync::Arc};

use cyancia_utils::wrapper;
use glam::UVec2;
use image::DynamicImage;
use uuid::Uuid;
use wgpu::TextureFormat;

use crate::{blend_modes::BlendMode, composite::BlendFunction, tile::GpuTileStorage};

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
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub LayerId : Uuid
}

#[derive(Clone)]
pub struct Layer {
    id: LayerId,
    pub name: String,
    pub blend_func: Box<dyn BlendFunction>,
}

impl std::fmt::Debug for Layer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Layer")
            .field("id", &self.id)
            .field("name", &self.name)
            .field("blend_func", &self.blend_func.name())
            .finish()
    }
}

impl Layer {
    pub fn new(name: String, blend_func: Box<dyn BlendFunction>) -> Self {
        Self {
            id: LayerId::new(Uuid::new_v4()),
            name,
            blend_func,
        }
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
        }
    }
}

#[derive(Debug, Clone)]
pub struct LayerStack {
    root: LayerStackNode,
    layers: HashMap<LayerId, Layer>,
}

impl LayerStack {
    pub fn new() -> Self {
        let background = Layer::new("Background".to_string(), Box::new(BlendMode::Normal));
        Self::with_background_layer(background)
    }

    pub fn with_background_layer(background: Layer) -> Self {
        let root = Layer::new("Root".to_string(), Box::new(BlendMode::Normal));
        let mut root_node = LayerStackNode::new(root.id, None);
        let background_node = LayerStackNode::new(background.id, Some(root.id));
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

    pub fn add_layer(&mut self, parent_id: LayerId, layer: Layer) {
        let parent_node = self.find_node_mut(parent_id);
        if let Some(parent_node) = parent_node {
            parent_node.insert_foreground_child(LayerStackNode::new(layer.id, Some(parent_id)));
            self.layers.insert(layer.id, layer);
        }
    }

    pub fn remove_layer(&mut self, layer_id: LayerId) -> Option<Layer> {
        let node = self.find_node(layer_id)?;
        if let Some(parent) = node
            .parent()
            .and_then(|parent_id| self.find_node_mut(parent_id))
        {
            parent.remove_child(layer_id);
        }

        self.layers.remove(&layer_id)
    }

    pub fn find_node(&self, layer_id: LayerId) -> Option<&LayerStackNode> {
        find_node_recursive(&self.root, layer_id)
    }

    pub fn find_node_mut(&mut self, layer_id: LayerId) -> Option<&mut LayerStackNode> {
        find_node_mut_recursive(&mut self.root, layer_id)
    }

    pub fn get_layer(&self, layer_id: LayerId) -> Option<&Layer> {
        self.layers.get(&layer_id)
    }

    pub fn get_layer_mut(&mut self, layer_id: LayerId) -> Option<&mut Layer> {
        self.layers.get_mut(&layer_id)
    }

    pub fn iter_layers_dfs_without_root(&self) -> impl Iterator<Item = &Layer> {
        let mut stack = self.root.children().iter().collect::<Vec<_>>();
        std::iter::from_fn(move || {
            let node = stack.pop()?;
            stack.extend(node.children().iter().rev());
            self.layers.get(&node.id())
        })
    }

    pub fn iter_layers(&self) -> impl Iterator<Item = &Layer> {
        self.layers.values()
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
    // - Parent
    //   - Child 1
    //   - Child 2
    //   - Child 3
    // When compositing, we render the Child 3 first, then 2, finally 1.
    // In this vector, the order of nodes are in render order.
    children: Vec<LayerStackNode>,
}

impl LayerStackNode {
    pub fn new(id: LayerId, parent: Option<LayerId>) -> Self {
        Self {
            id,
            parent,
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

    pub fn insert_background_child(&mut self, child: LayerStackNode) {
        self.children.insert(0, child);
    }

    pub fn insert_foreground_child(&mut self, child: LayerStackNode) {
        self.children.push(child);
    }

    pub fn insert_child(&mut self, index: usize, child: LayerStackNode) {
        self.children.insert(index, child);
    }

    pub fn remove_child(&mut self, child_id: LayerId) {
        self.children.retain(|child| child.id() != child_id);
    }
}
