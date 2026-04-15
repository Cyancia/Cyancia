use std::collections::HashMap;

use cyancia_utils::wrapper;
use glam::UVec2;
use image::DynamicImage;
use uuid::Uuid;
use wgpu::TextureFormat;

use crate::tile::GpuTileStorage;

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

#[derive(Debug)]
pub struct Layer {
    id: LayerId,
    name: String,
}

impl Layer {
    pub fn new(name: String) -> Self {
        Self {
            id: LayerId::new(Uuid::new_v4()),
            name,
        }
    }

    pub fn id(&self) -> LayerId {
        self.id
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn set_name(&mut self, name: String) {
        self.name = name;
    }

    pub fn from_image(name: String, img: DynamicImage, tiles: &GpuTileStorage) -> Self {
        let id = LayerId::new(Uuid::new_v4());
        tiles.upload_image(id, img);

        Self { id, name }
    }
}

#[derive(Debug)]
pub struct LayerStack {
    root: LayerStackNode,
    layers: HashMap<LayerId, Layer>,
}

impl LayerStack {
    pub fn new() -> Self {
        let root = Layer::new("Root".to_string());

        Self {
            root: LayerStackNode::new(root.id, None),
            layers: HashMap::from([(root.id, root)]),
        }
    }

    pub fn root(&self) -> LayerId {
        self.root.id
    }

    pub fn add_layer(&mut self, parent_id: LayerId, layer: Layer) {
        let parent_node = self.find_node_mut(parent_id);
        if let Some(parent_node) = parent_node {
            parent_node.add_child(LayerStackNode::new(layer.id, Some(parent_id)));
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

#[derive(Debug)]
pub struct LayerStackNode {
    id: LayerId,
    parent: Option<LayerId>,
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

    pub fn add_child(&mut self, child: LayerStackNode) {
        self.children.push(child);
    }

    pub fn remove_child(&mut self, child_id: LayerId) {
        self.children.retain(|child| child.id() != child_id);
    }
}
