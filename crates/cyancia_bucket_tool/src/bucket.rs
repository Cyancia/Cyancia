use cyancia_image::{
    texel::TexelType,
    tile::{DynamicLayerStorage, GpuTileInfo, GpuTileStorageInner, LayerBindingData},
};
use cyancia_render::buffer::DynamicBuffer;
use encase::{DynamicUniformBuffer, ShaderType};
use glam::{UVec2, Vec4};
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
    pub fill_color: Vec4,
    pub threshold: f32,
    pub alpha_threshold: f32,
}

pub struct PreparedBucket {
    input_layer_tile_count: u32,
    params_buffer: Buffer,
    bit_mask: Buffer,
    thresholding_bind_group: BindGroup,
    ccl_labels: Buffer,
    ccl_output: Buffer,
    ccl_bind_group: BindGroup,
    total_pixels: u32,
    composite_bind_group: BindGroup,
}

pub struct Bucket {
    thresholding_layout: BindGroupLayout,
    thresholding_pipeline: ComputePipeline,
    ccl_layout: BindGroupLayout,
    ccl_init_pipeline: ComputePipeline,
    ccl_merge_pipeline: ComputePipeline,
    ccl_compress_pipeline: ComputePipeline,
    ccl_extract_pipeline: ComputePipeline,
    composite_layout: BindGroupLayout,
    composite_pipeline: ComputePipeline,
}

impl Bucket {
    pub fn new(device: &Device, ref_texel_type: TexelType, output_texel_type: TexelType) -> Self {
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
                        format: ref_texel_type.wgpu_format(),
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

        let ccl_layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some("ccl_layout"),
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
                    ty: BindingType::Buffer {
                        ty: BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: Some(u32::min_size()),
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
                        ty: BufferBindingType::Storage { read_only: false },
                        has_dynamic_offset: false,
                        min_binding_size: Some(u32::min_size()),
                    },
                    count: None,
                },
                BindGroupLayoutEntry {
                    binding: 4,
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

        let ccl_pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: Some("ccl_pipeline_layout"),
            bind_group_layouts: &[&ccl_layout],
            push_constant_ranges: &[],
        });

        let ccl_shader = device.create_shader_module(ShaderModuleDescriptor {
            label: Some("ccl_shader"),
            source: ShaderSource::Wgsl(include_wesl!("ccl").into()),
        });

        let ccl_init_pipeline = device.create_compute_pipeline(&ComputePipelineDescriptor {
            label: Some("ccl_init_pipeline"),
            layout: Some(&ccl_pipeline_layout),
            module: &ccl_shader,
            entry_point: Some("ccl_init"),
            compilation_options: Default::default(),
            cache: None,
        });

        let ccl_merge_pipeline = device.create_compute_pipeline(&ComputePipelineDescriptor {
            label: Some("ccl_merge_pipeline"),
            layout: Some(&ccl_pipeline_layout),
            module: &ccl_shader,
            entry_point: Some("ccl_merge"),
            compilation_options: Default::default(),
            cache: None,
        });

        let ccl_compress_pipeline = device.create_compute_pipeline(&ComputePipelineDescriptor {
            label: Some("ccl_compress_pipeline"),
            layout: Some(&ccl_pipeline_layout),
            module: &ccl_shader,
            entry_point: Some("ccl_compress"),
            compilation_options: Default::default(),
            cache: None,
        });

        let ccl_extract_pipeline = device.create_compute_pipeline(&ComputePipelineDescriptor {
            label: Some("ccl_extract_pipeline"),
            layout: Some(&ccl_pipeline_layout),
            module: &ccl_shader,
            entry_point: Some("ccl_extract"),
            compilation_options: Default::default(),
            cache: None,
        });

        let composite_layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some("composite_layout"),
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
                    ty: BindingType::Buffer {
                        ty: BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: Some(u32::min_size()),
                    },
                    count: None,
                },
                BindGroupLayoutEntry {
                    binding: 2,
                    visibility: ShaderStages::COMPUTE,
                    ty: BindingType::StorageTexture {
                        access: StorageTextureAccess::ReadWrite,
                        format: output_texel_type.wgpu_format(),
                        view_dimension: TextureViewDimension::D2Array,
                    },
                    count: None,
                },
            ],
        });

        let composite_pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: Some("composite_pipeline_layout"),
            bind_group_layouts: &[&composite_layout],
            push_constant_ranges: &[],
        });
        let composite_shader = device.create_shader_module(ShaderModuleDescriptor {
            label: Some("composite_shader"),
            source: ShaderSource::Wgsl(include_wesl!("composite").into()),
        });
        let composite_pipeline = device.create_compute_pipeline(&ComputePipelineDescriptor {
            label: Some("composite_pipeline"),
            layout: Some(&composite_pipeline_layout),
            module: &composite_shader,
            entry_point: Some("main"),
            compilation_options: Default::default(),
            cache: None,
        });

        Self {
            thresholding_layout,
            thresholding_pipeline,
            ccl_layout,
            ccl_init_pipeline,
            ccl_merge_pipeline,
            ccl_compress_pipeline,
            ccl_extract_pipeline,
            composite_layout,
            composite_pipeline,
        }
    }

    pub fn prepare(
        &self,
        device: &Device,
        queue: &Queue,
        params: &BucketParams,
        ref_layer: &LayerBindingData,
        output_layer: &LayerBindingData,
    ) -> PreparedBucket {
        let input_layer_tile_count = ref_layer.texture.texture().depth_or_array_layers();

        let mut params_buffer =
            DynamicBuffer::new(Some("bucket_params_buffer"), BufferUsages::UNIFORM);
        params_buffer.push(params);
        params_buffer.write_buffer(device, queue);

        let bit_mask_size = (input_layer_tile_count
            * GpuTileStorageInner::TILE_SIZE
            * GpuTileStorageInner::TILE_SIZE
            + 31)
            / 32;

        let thresholded_output = device.create_buffer(&BufferDescriptor {
            label: Some("thresholded_output"),
            size: (bit_mask_size * 4) as u64,
            usage: BufferUsages::STORAGE | BufferUsages::COPY_SRC,
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
                    resource: BindingResource::TextureView(&ref_layer.texture),
                },
                BindGroupEntry {
                    binding: 2,
                    resource: thresholded_output.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 3,
                    resource: ref_layer.tile_info_buffer.as_entire_binding(),
                },
            ],
        });

        let total_pixels = input_layer_tile_count
            * GpuTileStorageInner::TILE_SIZE
            * GpuTileStorageInner::TILE_SIZE;

        let ccl_labels = device.create_buffer(&BufferDescriptor {
            label: Some("ccl_labels"),
            size: (total_pixels * 4) as u64,
            usage: BufferUsages::STORAGE,
            mapped_at_creation: false,
        });

        let ccl_output = device.create_buffer(&BufferDescriptor {
            label: Some("ccl_output"),
            size: (bit_mask_size * 4) as u64,
            usage: BufferUsages::STORAGE | BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });

        let ccl_bind_group = device.create_bind_group(&BindGroupDescriptor {
            label: Some("ccl_bind_group"),
            layout: &self.ccl_layout,
            entries: &[
                BindGroupEntry {
                    binding: 0,
                    resource: params_buffer.binding().unwrap(),
                },
                BindGroupEntry {
                    binding: 1,
                    resource: thresholded_output.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 2,
                    resource: ccl_labels.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 3,
                    resource: ccl_output.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 4,
                    resource: ref_layer.tile_info_buffer.as_entire_binding(),
                },
            ],
        });

        let composite_bind_group = device.create_bind_group(&BindGroupDescriptor {
            label: Some("composite_bind_group"),
            layout: &self.composite_layout,
            entries: &[
                BindGroupEntry {
                    binding: 0,
                    resource: params_buffer.binding().unwrap(),
                },
                BindGroupEntry {
                    binding: 1,
                    resource: thresholded_output.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 2,
                    resource: BindingResource::TextureView(&output_layer.texture),
                },
            ],
        });

        PreparedBucket {
            input_layer_tile_count,
            params_buffer: params_buffer.into_inner_buffer().unwrap(),
            bit_mask: thresholded_output,
            thresholding_bind_group,
            ccl_labels,
            ccl_output,
            ccl_bind_group,
            total_pixels,
            composite_bind_group,
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

        // unsafe { device.start_graphics_debugger_capture() };
        // debug_bit_mask(
        //     device,
        //     queue,
        //     &prepared.bit_mask,
        //     prepared.input_layer_tile_count,
        // );
        // unsafe { device.stop_graphics_debugger_capture() };

        let max_distance = (prepared.total_pixels as f32).sqrt().ceil() as u32;
        let iterations = (max_distance as f32).log2().ceil() as u32 + 1;

        let mut ec = device.create_command_encoder(&Default::default());

        {
            let mut pass = ec.begin_compute_pass(&ComputePassDescriptor {
                label: Some("ccl_init_pass"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&self.ccl_init_pipeline);
            pass.set_bind_group(0, &prepared.ccl_bind_group, &[]);
            pass.dispatch_workgroups(dispatch_xy, dispatch_xy, prepared.input_layer_tile_count);
        }

        {
            let mut pass = ec.begin_compute_pass(&ComputePassDescriptor {
                label: Some("ccl_merge_pass"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&self.ccl_merge_pipeline);
            pass.set_bind_group(0, &prepared.ccl_bind_group, &[]);
            for _ in 0..iterations {
                pass.dispatch_workgroups(dispatch_xy, dispatch_xy, prepared.input_layer_tile_count);
            }
        }

        {
            let mut pass = ec.begin_compute_pass(&ComputePassDescriptor {
                label: Some("ccl_compress_pass"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&self.ccl_compress_pipeline);
            pass.set_bind_group(0, &prepared.ccl_bind_group, &[]);
            pass.dispatch_workgroups(dispatch_xy, dispatch_xy, prepared.input_layer_tile_count);
        }

        {
            let mut pass = ec.begin_compute_pass(&ComputePassDescriptor {
                label: Some("ccl_extract_pass"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&self.ccl_extract_pipeline);
            pass.set_bind_group(0, &prepared.ccl_bind_group, &[]);
            pass.dispatch_workgroups(dispatch_xy, dispatch_xy, prepared.input_layer_tile_count);
        }

        {
            let mut pass = ec.begin_compute_pass(&ComputePassDescriptor {
                label: Some("composite_pass"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&self.composite_pipeline);
            pass.set_bind_group(0, &prepared.composite_bind_group, &[]);
            pass.dispatch_workgroups(dispatch_xy, dispatch_xy, prepared.input_layer_tile_count);
        }

        unsafe { device.start_graphics_debugger_capture() };
        queue.submit([ec.finish()]);
        unsafe { device.stop_graphics_debugger_capture() };

        // unsafe { device.start_graphics_debugger_capture() };
        // debug_bit_mask(
        //     device,
        //     queue,
        //     &prepared.ccl_output,
        //     prepared.input_layer_tile_count,
        // );
        // unsafe { device.stop_graphics_debugger_capture() };
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
