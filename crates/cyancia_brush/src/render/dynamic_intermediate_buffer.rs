use std::sync::Arc;

use cyancia_image::{
    texel::TexelType,
    tile::{GpuTileInfo, GpuTileStorageInner, LayerBindingData},
};
use cyancia_render::buffer::{BufferVec, DynamicBuffer};
use encase::ShaderType;
use glam::IVec2;
use wgpu::{
    Buffer, BufferUsages, Device, Extent3d, Queue, Texture, TextureDescriptor, TextureDimension, TextureFormat, TextureUsages, TextureView
};

#[derive(ShaderType)]
pub struct DynamicGpuTileInfoBuffer {
    pub n_tiles: u32,
    #[shader(size(runtime))]
    pub buf: Vec<GpuTileInfo>,
}

pub struct DynamicIntermediateBuffer {
    device: Device,
    queue: Queue,
    textures: [TextureView; 2],
    tile_info: DynamicBuffer<DynamicGpuTileInfoBuffer>,
    texel_type: TexelType,
    current: usize,
}

impl DynamicIntermediateBuffer {
    pub fn new(initial: u32, texel_type: TexelType, device: Device, queue: Queue) -> Self {
        let desc = TextureDescriptor {
            label: Some("dynamic intermediate buffer texture"),
            size: Extent3d {
                width: GpuTileStorageInner::TILE_SIZE,
                height: GpuTileStorageInner::TILE_SIZE,
                depth_or_array_layers: initial,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: TextureDimension::D2,
            format: texel_type.wgpu_format(),
            usage: TextureUsages::COPY_DST
                | TextureUsages::COPY_SRC
                | TextureUsages::STORAGE_BINDING,
            view_formats: &[],
        };
        let texture_a = device
            .create_texture(&desc)
            .create_view(&Default::default());
        let texture_b = device
            .create_texture(&desc)
            .create_view(&Default::default());

        let mut info = DynamicBuffer::new(
            Some("dynamic intermediate buffer".into()),
            BufferUsages::STORAGE | BufferUsages::COPY_SRC,
        );
        info.push(&DynamicGpuTileInfoBuffer {
            n_tiles: 0,
            buf: vec![GpuTileInfo::NULL; initial as usize],
        });
        info.write_buffer(&device, &queue);

        Self {
            device,
            queue,
            textures: [texture_a, texture_b],
            tile_info: info,
            texel_type,
            current: 0,
        }
    }

    pub fn src_tex(&self) -> TextureView {
        self.textures[self.current].clone()
    }

    pub fn dst_tex(&self) -> TextureView {
        self.textures[1 - self.current].clone()
    }

    pub fn textures(&self) -> &[TextureView; 2] {
        &self.textures
    }

    pub fn tile_info_buffer(&self) -> &Buffer {
        self.tile_info.inner_buffer().unwrap()
    }

    pub fn swap(&mut self) {
        self.current = 1 - self.current;
    }
}
