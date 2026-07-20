use std::{
    collections::HashMap,
    io::{Read, Write},
};

use anyhow::{Result, anyhow};
use cyancia_cyan::{CyanArchive, ImageProperties, LayerNode};
use cyancia_render::render_context::{RenderContext, RenderContextAppExt};
use flate2::{Compression, read::DeflateDecoder, write::DeflateEncoder};
use glam::{IVec2, UVec2};
use gpui::{App, AsyncApp};
use indexmap::IndexMap;
use moxcms::ColorProfile;
use serde::Serialize;
use uuid::Uuid;
use wgpu::{Device, Queue};

use crate::{
    CImage,
    layer::{
        Layer, LayerId, LayerStack, LayerStackNode, LayerTypeRegistry, SpecialLayers,
        properties::{
            EncodedLayerProperties, HasLayerProperties, LayerProperties, LayerTexelTypePropertyExt,
        },
    },
    tile::{DynamicLayerStorage, GpuLayerInfo, GpuTileStorage, TileStorageAppExt},
};

impl CImage {
    pub fn read_archive(archive: &CyanArchive, cx: &App) -> Result<Self> {
        let image_props = archive.read_image_properties()?;
        let layer_stack =
            LayerStack::read_entire_tree(image_props.root_layer, archive, cx.global())?;

        let queue = cx.render_queue();
        let tile_storage = cx.tile_storage();
        for layer in layer_stack.iter_layers() {
            let Some(texel_type) = layer.properties().get_texel_type() else {
                continue;
            };

            let tile_data = archive.read_layer_data(**layer.id())?;
            tile_storage.declare_layer(*layer.id(), GpuLayerInfo { texel_type });
            let mut layer = tile_storage.get_layer_mut(*layer.id()).unwrap();
            layer.allocate_tiles_batch(tile_data.keys());

            for (index, data) in tile_data {
                layer.write_raw(queue, index, &data);
            }
        }

        let image_texel_type = layer_stack
            .root_node()
            .properties()
            .get_texel_type()
            .unwrap();

        Ok(Self {
            size: UVec2::new(image_props.width, image_props.height),
            profile: ColorProfile::new_from_slice(&image_props.color_profile)?,
            texel_type: image_texel_type,
            layers: layer_stack,
            name_generator: Default::default(),
            special_layers: SpecialLayers::new(),
        })
    }

    pub async fn write_archive(&self, archive: &CyanArchive, cx: &App) -> Result<()> {
        archive.write_image_properties(&ImageProperties {
            width: self.size.x,
            height: self.size.y,
            tile_size: GpuTileStorage::TILE_SIZE,
            color_profile: self.profile.encode()?,
            root_layer: **self.layers.root_node().id(),
        })?;

        self.layers.write_entire_tree(archive)?;
        let render_context = cx.render_context();
        let tile_storage = cx.tile_storage();

        let result = futures::future::join_all(
            self.layers
                .iter_layers()
                .filter(|layer| layer.can_contain_pixels())
                .map(|layer| {
                    tile_storage.write_layer(
                        &render_context.device,
                        &render_context.queue,
                        archive,
                        *layer.id(),
                    )
                }),
        )
        .await;
        for r in result {
            r?;
        }

        Ok(())
    }
}

impl GpuTileStorage {
    pub async fn write_layer(
        &self,
        device: &Device,
        queue: &Queue,
        archive: &CyanArchive,
        layer_id: LayerId,
    ) -> Result<()> {
        let layer = self
            .get_layer(layer_id)
            .ok_or_else(|| anyhow!("layer {} doesn't exists", layer_id))?;

        let tile_data = layer
            .readback(device, queue, layer.iter_tile_indices())
            .await?;

        for (tile, data) in tile_data {
            Self::write_tile_data(archive, layer_id, tile, &data)?;
        }

        Ok(())
    }

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
            Self::write_tile_data(archive, layer_id, tile, &data)?;
        }

        Ok(())
    }

    fn write_tile_data(
        archive: &CyanArchive,
        layer_id: LayerId,
        tile: IVec2,
        data: &[u8],
    ) -> Result<()> {
        let mut e = DeflateEncoder::new(Vec::new(), Compression::default());
        e.write_all(&data)?;
        let buf = e.finish()?;
        archive.write_tile_data(layer_id.into_inner(), tile.x, tile.y, buf)?;
        Ok(())
    }

    pub fn read_tiles(
        &self,
        queue: &Queue,
        archive: &CyanArchive,
        layer_id: Uuid,
        tiles: Vec<IVec2>,
    ) -> Result<()> {
        let mut layer = self
            .get_layer_mut(LayerId(layer_id))
            .ok_or_else(|| anyhow!("layer {} doesn't exists", layer_id))?;

        layer.allocate_tiles_batch(tiles.iter().copied());

        for index in tiles {
            let tile = archive
                .read_tile_data(layer_id, index.x, index.y)?
                .ok_or_else(|| anyhow!("tile ({}, {}) not found", index.x, index.y))?;
            let mut d = DeflateDecoder::new(&tile[..]);
            let mut buf = Vec::new();
            d.read_to_end(&mut buf)?;
            layer.write_raw(queue, index, &buf);
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
