use std::{collections::HashMap, io::Write};

use anyhow::{Result, anyhow};
use cyancia_cyan::CyanArchive;
use flate2::{Compression, write::DeflateEncoder};
use glam::IVec2;
use serde::Serialize;
use wgpu::{Device, Queue};

use crate::{
    layer::{
        Layer, LayerId, LayerStack,
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

    pub fn decode<T: HasLayerProperties>(&mut self, data: &[u8]) -> Result<Self> {
        let map = EncodedLayerProperties::new(rmp_serde::from_slice::<
            HashMap<String, Vec<u8>>,
        >(data)?);
        Self::new_decoded::<T>(map)
    }
}
