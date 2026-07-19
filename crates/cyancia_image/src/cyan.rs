use std::{collections::HashMap, io::Write};

use anyhow::{Result, anyhow};
use cyancia_cyan::{CyanArchive, LayerNode};
use flate2::{Compression, write::DeflateEncoder};
use glam::IVec2;
use indexmap::IndexMap;
use serde::Serialize;
use uuid::Uuid;
use wgpu::{Device, Queue};

use crate::{
    layer::{
        Layer, LayerId, LayerStack, LayerStackNode, LayerTypeRegistry,
        properties::{EncodedLayerProperties, HasLayerProperties, LayerProperties},
    },
    tile::{DynamicLayerStorage, GpuTileStorage},
};

impl GpuTileStorage {
    pub async fn write_tiles(
        &self,
        device: &Device,
        queue: &Queue,
        archive: &CyanArchive,
        layer_id: LayerId,
        tiles: impl IntoIterator<Item = IVec2>,
    ) -> Result<()> {
        let layer = self
            .get_layer(layer_id)
            .ok_or_else(|| anyhow!("layer {} doesn't exists", layer_id))?;
        let tile_data = layer.readback(device, queue, tiles).await?;

        for (tile, data) in tile_data {
            let mut e = DeflateEncoder::new(Vec::new(), Compression::default());
            e.write_all(&data)?;
            let buf = e.finish()?;
            archive.write_tile_data(layer_id.into_inner(), tile.x, tile.y, buf)?;
        }

        Ok(())
    }
}

impl Serialize for LayerProperties {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let encoded = self
            .iter()
            .map(|(k, v)| Ok((k, v.encode()?)))
            .collect::<Result<HashMap<_, _>>>()
            .map_err(|e| <S::Error as serde::ser::Error>::custom(e))?;
        encoded.serialize(serializer)
    }
}

impl LayerProperties {
    pub fn encode(&self) -> Result<Vec<u8>> {
        Ok(rmp_serde::to_vec(self)?)
    }

    pub fn decode(data: &[u8], layer: &dyn Layer) -> Result<Self> {
        Self::new_decoded(EncodedLayerProperties::new(data)?, layer)
    }
}

impl LayerStack {
    pub fn write_entire_tree(&self, archive: &CyanArchive) -> Result<()> {
        for layer in self.iter_layers() {
            let (parent_id, sort_order) = match layer.parent() {
                Some(parent_id) => {
                    let parent = self.get_layer(parent_id).unwrap();
                    let sort_order = parent.child_index(layer.id()).unwrap() as u32;
                    (Some(**parent.id()), Some(sort_order))
                }
                None => (None, None),
            };

            archive.write_layer_node(&LayerNode {
                id: **layer.id(),
                parent_id,
                sort_order,
                layer_type: layer.instance().layer_type(),
                properties: layer.properties().encode()?,
            })?;
        }
        Ok(())
    }

    pub fn read_entire_tree(
        root_layer: Uuid,
        archive: &CyanArchive,
        layer_types: &LayerTypeRegistry,
    ) -> Result<Self> {
        let mut nodes = archive.read_all_layer_nodes()?;
        nodes.sort_by_key(|n| n.sort_order);

        let mut root_node = None;
        let mut nodes_map = IndexMap::with_capacity(nodes.len() - 1);
        for node in nodes.into_iter().rev() {
            if node.id == root_layer {
                root_node = Some(node);
            } else {
                nodes_map.insert(node.id, node);
            }
        }

        let root_node = root_node.ok_or(anyhow::anyhow!("root layer not found"))?;

        fn read_node(
            id: &Uuid,
            nodes: &mut IndexMap<Uuid, LayerNode>,
            output: &mut LayerStack,
            layer_types: &LayerTypeRegistry,
        ) -> Result<()> {
            let node = nodes.shift_remove(id).unwrap();
            if let Some(parent_id) = &node.parent_id
                && !output.contains_layer(&LayerId(*parent_id))
            {
                read_node(parent_id, nodes, output, layer_types)?;
            }

            let instance = layer_types
                .get_cloned(node.layer_type)
                .ok_or(anyhow::anyhow!("Unknown layer type: {}", node.layer_type))?;
            let props = LayerProperties::decode(&node.properties, instance.as_ref())?;

            if let Some(parent_id) = &node.parent_id {
                let parent_id = LayerId(*parent_id);
                output.add_layer(
                    parent_id,
                    node.sort_order.unwrap() as usize,
                    LayerStackNode::without_parent(LayerId(*id), instance, props),
                );
            }

            Ok(())
        }

        let instance = layer_types
            .get_cloned(root_node.layer_type)
            .ok_or(anyhow::anyhow!(
                "Unknown layer type: {}",
                root_node.layer_type
            ))?;
        let props = LayerProperties::decode(&root_node.properties, instance.as_ref())?;
        let root_node = LayerStackNode::without_parent(LayerId(root_layer), instance, props);
        let mut output = LayerStack::new(root_node);

        while let Some(id) = nodes_map.keys().last().copied() {
            read_node(&id, &mut nodes_map, &mut output, layer_types)?;
        }

        Ok(output)
    }
}
