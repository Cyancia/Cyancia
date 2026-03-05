use std::marker::PhantomData;

use encase::{
    ShaderType,
    internal::{WriteInto, Writer},
};
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

impl<T: ShaderType + WriteInto> Default for DynamicBuffer<T> {
    fn default() -> Self {
        Self {
            label: Default::default(),
            usage: BufferUsages::COPY_DST,
            buffer: Default::default(),
            wrapper: encase::DynamicStorageBuffer::new(Vec::new()),
            _marker: Default::default(),
        }
    }
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

    pub fn push(&mut self, data: &T) -> BufferAddress {
        self.wrapper.write(data).unwrap()
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
            size: None,
        }))
    }

    pub fn clear(&mut self) {
        self.wrapper.as_mut().clear();
        self.wrapper.set_offset(0);
        self.buffer = None;
    }

    pub fn usage(&self) -> BufferUsages {
        self.usage
    }

    pub fn usage_mut(&mut self) -> &mut BufferUsages {
        &mut self.usage
    }

    pub fn with_usage(mut self, usage: BufferUsages) -> Self {
        self.usage = usage;
        self
    }
}

pub struct BufferVec<T: ShaderType + WriteInto> {
    label: Option<String>,
    data: Vec<u8>,
    buffer: Option<Buffer>,
    usage: BufferUsages,
    _marker: PhantomData<T>,
}

impl<T: ShaderType + WriteInto> Default for BufferVec<T> {
    fn default() -> Self {
        Self {
            label: Default::default(),
            data: Default::default(),
            buffer: Default::default(),
            usage: BufferUsages::COPY_DST,
            _marker: Default::default(),
        }
    }
}

impl<T: ShaderType + WriteInto> BufferVec<T> {
    pub fn new(label: Option<String>, usage: BufferUsages) -> Self {
        Self {
            label,
            data: Vec::new(),
            buffer: None,
            usage,
            _marker: PhantomData,
        }
    }

    pub fn push(&mut self, item: &T) -> usize {
        let element_size = u64::from(T::min_size()) as usize;
        let offset = self.data.len();

        // `extend` does not optimize for reallocation. Related `trusted_len` feature is unstable.
        self.data.reserve(self.data.len() + element_size);
        // We can't optimize and push uninitialized data here (using e.g. spare_capacity_mut())
        // because write_into() does not initialize inner padding bytes in T's expansion
        self.data.extend(std::iter::repeat_n(0, element_size));

        // Take a slice of the new data for `write_into` to use. This is
        // important: it hoists the bounds check up here so that the compiler
        // can eliminate all the bounds checks that `write_into` will emit.
        let mut dest = &mut self.data[offset..(offset + element_size)];
        item.write_into(&mut Writer::new(item, &mut dest, 0).unwrap());

        offset / u64::from(T::min_size()) as usize
    }

    pub fn write_buffer(&mut self, device: &Device) {
        let buffer = device.create_buffer_init(&BufferInitDescriptor {
            label: self.label.as_deref(),
            contents: &self.data,
            usage: self.usage,
        });
        self.buffer = Some(buffer);
    }

    pub fn binding(&self) -> Option<BindingResource<'_>> {
        Some(BindingResource::Buffer(
            self.buffer.as_ref()?.as_entire_buffer_binding(),
        ))
    }

    pub fn clear(&mut self) {
        self.data.clear();
        self.buffer = None;
    }

    pub fn usage(&self) -> BufferUsages {
        self.usage
    }

    pub fn usage_mut(&mut self) -> &mut BufferUsages {
        &mut self.usage
    }

    pub fn with_usage(mut self, usage: BufferUsages) -> Self {
        self.usage = usage;
        self
    }
}
