use std::marker::PhantomData;

use encase::{ShaderType, internal::WriteInto};
use wgpu::{
    BindingResource, Buffer, BufferAddress, BufferBinding, BufferUsages, Device,
    util::{BufferInitDescriptor, DeviceExt},
};

pub struct DynamicBuffer<T: ShaderType + WriteInto> {
    label: Option<&'static str>,
    usage: BufferUsages,
    buffer: Option<Buffer>,
    wrapper: encase::DynamicStorageBuffer<Vec<u8>>,
    _marker: PhantomData<T>,
}

impl<T: ShaderType + WriteInto> std::fmt::Debug for DynamicBuffer<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DynamicBuffer")
            .field("label", &self.label)
            .field("usage", &self.usage)
            .field(
                "buffer",
                &format!(
                    "{} bytes",
                    self.buffer.as_ref().map(|b| b.size()).unwrap_or(0)
                ),
            )
            .finish()
    }
}

impl<T: ShaderType + WriteInto> DynamicBuffer<T> {
    pub fn new(label: Option<&'static str>, usage: BufferUsages) -> Self {
        Self {
            label,
            usage: BufferUsages::COPY_DST | usage,
            buffer: None,
            wrapper: encase::DynamicStorageBuffer::new(Vec::new()),
            _marker: PhantomData,
        }
    }

    pub fn push(&mut self, data: &T) -> Option<BufferAddress> {
        self.wrapper.write(data).ok()
    }

    pub fn write_buffer(&mut self, device: &Device) {
        let contents = self.wrapper.as_ref();
        let buffer = device.create_buffer_init(&BufferInitDescriptor {
            label: self.label,
            contents: &contents,
            usage: self.usage,
        });
        self.buffer = Some(buffer);
    }

    pub fn binding(&self) -> Option<BindingResource<'_>> {
        Some(BindingResource::Buffer(BufferBinding {
            buffer: self.buffer.as_ref()?,
            offset: 0,
            size: Some(<T as ShaderType>::min_size()),
        }))
    }

    pub fn entire_binding(&self) -> Option<BindingResource<'_>> {
        Some(BindingResource::Buffer(BufferBinding {
            buffer: self.buffer.as_ref()?,
            offset: 0,
            size: None,
        }))
    }

    pub fn clear(&mut self) {
        self.wrapper.as_mut().clear();
        self.wrapper.set_offset(0);
    }

    pub fn usage(&self) -> BufferUsages {
        self.usage
    }

    pub fn usage_mut(&mut self) -> &mut BufferUsages {
        &mut self.usage
    }
}
