use cyancia_image::{
    texel::TexelType,
    tile::{GpuTileInfo, GpuTileStorageInner},
};
use cyancia_render::buffer::{BufferVec, DynamicBuffer};
use encase::ShaderType;
use glam::IVec2;
use wgpu::{
    Buffer, BufferUsages, Device, Extent3d, Queue, Texture, TextureDescriptor, TextureDimension,
    TextureFormat, TextureUsages, TextureView,
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
    textures_inner: [Texture; 2],
    textures: [TextureView; 2],
    tile_info: DynamicBuffer<DynamicGpuTileInfoBuffer>,
    texel_type: TexelType,
    tiles: u32,
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

        let texture_a_raw = device.create_texture(&desc);
        let texture_b_raw = device.create_texture(&desc);
        let texture_a = texture_a_raw.create_view(&Default::default());
        let texture_b = texture_b_raw.create_view(&Default::default());

        let mut info = DynamicBuffer::new(
            Some("dynamic intermediate buffer".into()),
            BufferUsages::STORAGE | BufferUsages::COPY_SRC,
        );
        info.push(&DynamicGpuTileInfoBuffer {
            n_tiles: 0,
            buf: vec![GpuTileInfo::default(); initial as usize],
        });
        info.write_buffer(&device, &queue);

        Self {
            device,
            queue,
            textures_inner: [texture_a_raw, texture_b_raw],
            textures: [texture_a, texture_b],
            tile_info: info,
            texel_type,
            tiles: initial,
        }
    }

    pub fn textures(&self) -> &[TextureView; 2] {
        &self.textures
    }

    pub fn tile_info_buffer(&self) -> &Buffer {
        self.tile_info.inner_buffer().unwrap()
    }

    pub fn clear(&mut self) {
        // Clear the intermediate textures so stale pixel data from the previous
        // stroke cannot bleed into the next stroke via current_input_color().
        // New GPU textures are zero-initialised, but on reuse the ping-pong
        // buffers still hold whatever was written during the last stroke.
        // Without this clear the first dab of every subsequent stroke reads the
        // old texture content at the same array-layer index, causing the
        // previous stroke's tile pixels to be composited into the wrong canvas
        // position (e.g. tile (1,1) visually "flying" to position (1,3)).
        let mut ec = self
            .device
            .create_command_encoder(&Default::default());
        for texture in &self.textures_inner {
            ec.clear_texture(texture, &Default::default());
        }
        self.queue.submit([ec.finish()]);

        self.tile_info.clear();
        self.tile_info.push(&DynamicGpuTileInfoBuffer {
            n_tiles: 0,
            buf: vec![GpuTileInfo::NULL; self.tiles as usize],
        });
        self.tile_info.write_buffer(&self.device, &self.queue);
    }
}
