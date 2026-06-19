use cyancia_render::{
    readback::{create_readback_buffer_and_schedule_copy, readback_buffer_on_submit_async},
    util::DevicePollExt,
};
use encase::ShaderType;
use glam::IVec2;
use indexmap::IndexSet;
use wesl::include_wesl;
use wgpu::{
    BindGroupDescriptor, BindGroupEntry, BindGroupLayout, BindGroupLayoutDescriptor,
    BindGroupLayoutEntry, BindingResource, BindingType, BufferBindingType, BufferDescriptor,
    BufferUsages, ComputePassDescriptor, ComputePipeline, ComputePipelineDescriptor, Device,
    PipelineLayoutDescriptor, Queue, ShaderModuleDescriptor, ShaderSource, ShaderStages,
    StorageTextureAccess, TextureViewDimension,
};

use crate::{
    texel::TexelType,
    tile::{DynamicLayerStorage, GpuTileInfo, GpuTileStorageInner},
};

pub struct ScanPixelsPipeline {
    layout: BindGroupLayout,
    pipeline: ComputePipeline,
}

impl ScanPixelsPipeline {
    pub fn new(device: &Device, layer_format: TexelType) -> Self {
        let scan_pixels_layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some("scan_pixels_layout"),
            entries: &[
                BindGroupLayoutEntry {
                    binding: 0,
                    visibility: ShaderStages::COMPUTE,
                    ty: BindingType::StorageTexture {
                        access: StorageTextureAccess::ReadOnly,
                        format: layer_format.wgpu_format(),
                        view_dimension: TextureViewDimension::D2Array,
                    },
                    count: None,
                },
                BindGroupLayoutEntry {
                    binding: 1,
                    visibility: ShaderStages::COMPUTE,
                    ty: BindingType::Buffer {
                        ty: BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: Some(GpuTileInfo::min_size()),
                    },
                    count: None,
                },
                BindGroupLayoutEntry {
                    binding: 2,
                    visibility: ShaderStages::COMPUTE,
                    ty: BindingType::Buffer {
                        ty: BufferBindingType::Storage { read_only: false },
                        has_dynamic_offset: false,
                        min_binding_size: Some(u32::min_size()),
                    },
                    count: None,
                },
            ],
        });
        let scan_pixels_pipeline_layout =
            device.create_pipeline_layout(&PipelineLayoutDescriptor {
                label: Some("scan_pixels_pipeline_layout"),
                bind_group_layouts: &[Some(&scan_pixels_layout)],
                ..Default::default()
            });
        let scan_pixels_shader = device.create_shader_module(ShaderModuleDescriptor {
            label: Some("scan_pixels_shader"),
            source: ShaderSource::Wgsl(include_wesl!("scan_pixels").into()),
        });
        let scan_pixels_pipeline = device.create_compute_pipeline(&ComputePipelineDescriptor {
            label: Some("scan_pixels_pipeline"),
            layout: Some(&scan_pixels_pipeline_layout),
            module: &scan_pixels_shader,
            entry_point: Some("main"),
            compilation_options: Default::default(),
            cache: None,
        });

        Self {
            layout: scan_pixels_layout,
            pipeline: scan_pixels_pipeline,
        }
    }

    pub fn scan(
        &self,
        device: &Device,
        queue: &Queue,
        target_layer: &DynamicLayerStorage,
    ) -> IndexSet<IVec2> {
        let mut ec = device.create_command_encoder(&Default::default());

        let is_not_empty_buffer = device.create_buffer(&BufferDescriptor {
            label: Some("is_not_empty_buffer"),
            size: target_layer.len() as u64 * 4,
            usage: BufferUsages::STORAGE | BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });

        let scan_pixels_bind_group = device.create_bind_group(&BindGroupDescriptor {
            label: Some("scan_pixels_bind_group"),
            layout: &self.layout,
            entries: &[
                BindGroupEntry {
                    binding: 0,
                    resource: BindingResource::TextureView(&target_layer.texture().unwrap()),
                },
                BindGroupEntry {
                    binding: 1,
                    resource: target_layer.tile_info_buffer().unwrap().as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 2,
                    resource: is_not_empty_buffer.as_entire_binding(),
                },
            ],
        });

        {
            let mut pass = ec.begin_compute_pass(&ComputePassDescriptor {
                label: Some("scan_pixels_pass"),
                ..Default::default()
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &scan_pixels_bind_group, &[]);
            pass.dispatch_workgroups(
                GpuTileStorageInner::TILE_SIZE.div_ceil(16),
                GpuTileStorageInner::TILE_SIZE.div_ceil(16),
                target_layer.len() as u32,
            );
        }

        let is_not_empty_readback =
            create_readback_buffer_and_schedule_copy(device, &mut ec, &is_not_empty_buffer);
        let is_not_empty_readback_async =
            readback_buffer_on_submit_async::<Vec<u32>, _>(&mut ec, &is_not_empty_readback, ..);

        let si = queue.submit([ec.finish()]);
        device.poll_indefinitely_for(si).unwrap();

        let is_not_empty = is_not_empty_readback_async.block_on().unwrap();
        target_layer
            .iter_tiles()
            .zip(is_not_empty)
            .filter_map(|((i, _, _), is_not_empty)| if is_not_empty == 1 { Some(i) } else { None })
            .collect()
    }
}
