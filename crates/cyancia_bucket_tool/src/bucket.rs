use cyancia_image::{
    texel::TexelType,
    tile::{DynamicLayerStorage, GpuTileInfo, GpuTileStorageInner, LayerBindingData},
};
use cyancia_render::buffer::DynamicBuffer;
use encase::{DynamicUniformBuffer, ShaderType};
use glam::UVec2;
use wesl::include_wesl;
use wgpu::{
    BindGroup, BindGroupDescriptor, BindGroupEntry, BindGroupLayout, BindGroupLayoutDescriptor,
    BindGroupLayoutEntry, BindingResource, BindingType, Buffer, BufferBindingType, BufferUsages,
    ComputePassDescriptor, ComputePipeline, ComputePipelineDescriptor, Device, Extent3d,
    PipelineLayoutDescriptor, Queue, ShaderModuleDescriptor, ShaderSource, ShaderStages,
    StorageTextureAccess, TextureDimension, TextureFormat, TextureUsages, TextureView,
    TextureViewDimension, include_wgsl,
    wgt::{BufferDescriptor, TextureDescriptor},
};

#[derive(ShaderType, Debug, Clone, Copy)]
pub struct BucketParams {
    pub seed: UVec2,
    pub threshold: f32,
    pub alpha_threshold: f32,
}

pub struct PreparedBucket {
    input_layer_tile_count: u32,
    params_buffer: Buffer,
    bit_mask: Buffer,
    thresholding_bind_group: BindGroup,
}

pub struct Bucket {
    thresholding_layout: BindGroupLayout,
    thresholding_pipeline: ComputePipeline,
}

impl Bucket {
    pub fn new(device: &Device, input_texel_type: TexelType) -> Self {
        let thresholding_layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some("thresholding_layout"),
            entries: &[
                BindGroupLayoutEntry {
                    binding: 0,
                    visibility: ShaderStages::COMPUTE,
                    ty: BindingType::Buffer {
                        ty: BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: Some(BucketParams::min_size()),
                    },
                    count: None,
                },
                BindGroupLayoutEntry {
                    binding: 1,
                    visibility: ShaderStages::COMPUTE,
                    ty: BindingType::StorageTexture {
                        access: StorageTextureAccess::ReadOnly,
                        format: input_texel_type.wgpu_format(),
                        view_dimension: TextureViewDimension::D2Array,
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
                BindGroupLayoutEntry {
                    binding: 3,
                    visibility: ShaderStages::COMPUTE,
                    ty: BindingType::Buffer {
                        ty: BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: Some(GpuTileInfo::min_size()),
                    },
                    count: None,
                },
            ],
        });

        let thresholding_pipeline_layout =
            device.create_pipeline_layout(&PipelineLayoutDescriptor {
                label: Some("thresholding_pipeline_layout"),
                bind_group_layouts: &[&thresholding_layout],
                push_constant_ranges: &[],
            });
        let thresholding_shader = device.create_shader_module(ShaderModuleDescriptor {
            label: Some("thresholding_shader_module"),
            source: ShaderSource::Wgsl(include_wesl!("thresholding").into()),
        });
        let thresholding_pipeline = device.create_compute_pipeline(&ComputePipelineDescriptor {
            label: Some("thresholding_pipeline"),
            layout: Some(&thresholding_pipeline_layout),
            module: &thresholding_shader,
            entry_point: Some("main"),
            compilation_options: Default::default(),
            cache: None,
        });

        Self {
            thresholding_layout,
            thresholding_pipeline,
        }
    }

    pub fn prepare(
        &self,
        device: &Device,
        queue: &Queue,
        params: &BucketParams,
        input_layer: &LayerBindingData,
    ) -> PreparedBucket {
        let input_layer_tile_count = input_layer.texture.texture().depth_or_array_layers();

        let mut params_buffer =
            DynamicBuffer::new(Some("bucket_params_buffer"), BufferUsages::UNIFORM);
        params_buffer.push(params);
        params_buffer.write_buffer(device, queue);

        let thresholded_output = device.create_buffer(&BufferDescriptor {
            label: Some("thresholded_output"),
            size: (input_layer_tile_count
                * GpuTileStorageInner::TILE_SIZE
                * GpuTileStorageInner::TILE_SIZE) as u64,
            usage: BufferUsages::STORAGE,
            mapped_at_creation: false,
        });

        let thresholding_bind_group = device.create_bind_group(&BindGroupDescriptor {
            label: Some("thresholding_bind_group"),
            layout: &self.thresholding_layout,
            entries: &[
                BindGroupEntry {
                    binding: 0,
                    resource: params_buffer.binding().unwrap(),
                },
                BindGroupEntry {
                    binding: 1,
                    resource: BindingResource::TextureView(&input_layer.texture),
                },
                BindGroupEntry {
                    binding: 2,
                    resource: thresholded_output.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 3,
                    resource: input_layer.tile_info_buffer.as_entire_binding(),
                },
            ],
        });

        PreparedBucket {
            input_layer_tile_count,
            params_buffer: params_buffer.into_inner_buffer().unwrap(),
            bit_mask: thresholded_output,
            thresholding_bind_group,
        }
    }

    pub fn dispatch(&self, device: &Device, queue: &Queue, prepared: PreparedBucket) {
        let dispatch_xy = GpuTileStorageInner::TILE_SIZE.div_ceil(16);

        let mut ec = device.create_command_encoder(&Default::default());

        {
            let mut pass = ec.begin_compute_pass(&ComputePassDescriptor {
                label: Some("thresholding_pass"),
                timestamp_writes: None,
            });

            pass.set_pipeline(&self.thresholding_pipeline);
            pass.set_bind_group(0, &prepared.thresholding_bind_group, &[]);
            pass.dispatch_workgroups(dispatch_xy, dispatch_xy, prepared.input_layer_tile_count);
        }

        queue.submit([ec.finish()]);
        unsafe { device.start_graphics_debugger_capture() };
        debug_bit_mask(
            device,
            queue,
            &prepared.bit_mask,
            prepared.input_layer_tile_count,
        );
        unsafe { device.stop_graphics_debugger_capture() };
    }
}

fn debug_bit_mask(device: &Device, queue: &Queue, bit_mask: &Buffer, n_tiles: u32) {
    let debug_bit_mask_layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
        label: Some("debug_bit_mask_layout"),
        entries: &[
            BindGroupLayoutEntry {
                binding: 0,
                visibility: ShaderStages::COMPUTE,
                ty: BindingType::Buffer {
                    ty: BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: Some(u32::min_size()),
                },
                count: None,
            },
            BindGroupLayoutEntry {
                binding: 1,
                visibility: ShaderStages::COMPUTE,
                ty: BindingType::StorageTexture {
                    access: StorageTextureAccess::WriteOnly,
                    format: TextureFormat::Rgba8Unorm,
                    view_dimension: TextureViewDimension::D2Array,
                },
                count: None,
            },
        ],
    });

    let debug_bit_mask_pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
        label: Some("debug_bit_mask_pipeline_layout"),
        bind_group_layouts: &[&debug_bit_mask_layout],
        push_constant_ranges: &[],
    });
    let debug_bit_mask_shader = device.create_shader_module(ShaderModuleDescriptor {
        label: Some("debug_bit_mask_shader"),
        source: ShaderSource::Wgsl(include_wesl!("debug_bit_mask").into()),
    });
    let debug_bit_mask_pipeline = device.create_compute_pipeline(&ComputePipelineDescriptor {
        label: Some("debug_bit_mask_pipeline"),
        layout: Some(&debug_bit_mask_pipeline_layout),
        module: &debug_bit_mask_shader,
        entry_point: Some("main"),
        compilation_options: Default::default(),
        cache: None,
    });

    let output_tex = device.create_texture(&TextureDescriptor {
        label: Some("debug_bit_mask_output_tex"),
        size: Extent3d {
            width: GpuTileStorageInner::TILE_SIZE,
            height: GpuTileStorageInner::TILE_SIZE,
            depth_or_array_layers: n_tiles,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: TextureDimension::D2,
        format: TextureFormat::Rgba8Unorm,
        usage: TextureUsages::STORAGE_BINDING,
        view_formats: &[],
    });
    let bind_group = device.create_bind_group(&BindGroupDescriptor {
        label: Some("debug_bit_mask_bind_group"),
        layout: &debug_bit_mask_layout,
        entries: &[
            BindGroupEntry {
                binding: 0,
                resource: bit_mask.as_entire_binding(),
            },
            BindGroupEntry {
                binding: 1,
                resource: BindingResource::TextureView(
                    &output_tex.create_view(&Default::default()),
                ),
            },
        ],
    });

    let mut ec = device.create_command_encoder(&Default::default());
    {
        let mut pass = ec.begin_compute_pass(&ComputePassDescriptor {
            label: Some("debug_bit_mask_pass"),
            timestamp_writes: None,
        });
        pass.set_pipeline(&debug_bit_mask_pipeline);
        pass.set_bind_group(0, &bind_group, &[]);
        pass.dispatch_workgroups(
            GpuTileStorageInner::TILE_SIZE.div_ceil(16),
            GpuTileStorageInner::TILE_SIZE.div_ceil(16),
            n_tiles,
        );
    }
    queue.submit([ec.finish()]);
}
