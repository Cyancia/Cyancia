use std::io::Write;

use anyhow::{Result, anyhow};
use cyancia_cyan::CyanArchive;
use flate2::{Compression, write::DeflateEncoder};
use glam::IVec2;
use wgpu::{Device, Queue};

use crate::{
    layer::{LayerId, LayerStack},
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
