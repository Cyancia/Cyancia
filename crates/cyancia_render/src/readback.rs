use std::ops::RangeBounds;

use cyancia_utils::{Deref, DerefMut};
use encase::{ShaderType, internal::CreateFrom};
use futures::channel::oneshot::{Receiver, Sender};
use wgpu::{
    Buffer, BufferAddress, BufferAsyncError, BufferUsages, CommandEncoder, Device, MapMode,
};

pub fn create_readback_buffer_and_schedule_copy(
    device: &Device,
    ec: &mut CommandEncoder,
    src_buffer: &Buffer,
) -> Buffer {
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: src_buffer.size(),
        usage: BufferUsages::MAP_READ | BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    ec.copy_buffer_to_buffer(src_buffer, 0, &buffer, 0, src_buffer.size());
    buffer
}

#[derive(Deref, DerefMut)]
pub struct AsyncBufferReadback<T> {
    rx: Receiver<anyhow::Result<T>>,
}

impl<T> AsyncBufferReadback<T> {
    pub fn block_on(self) -> anyhow::Result<T> {
        futures::executor::block_on(self.rx)?
    }
}

pub fn readback_buffer_on_submit_async<T, S>(
    ec: &mut CommandEncoder,
    buffer: &Buffer,
    bounds: S,
) -> AsyncBufferReadback<T>
where
    T: ShaderType + CreateFrom + Send + 'static,
    S: RangeBounds<BufferAddress> + Clone + Send + 'static,
{
    let (tx, rx) = futures::channel::oneshot::channel();
    ec.map_buffer_on_submit(
        buffer,
        MapMode::Read,
        bounds.clone(),
        on_buffer_mapped(buffer.clone(), bounds, tx),
    );
    AsyncBufferReadback { rx }
}

pub fn readback_buffer_async<T, S>(buffer: &Buffer, bounds: S) -> AsyncBufferReadback<T>
where
    T: ShaderType + CreateFrom + Send + 'static,
    S: RangeBounds<BufferAddress> + Clone + Send + 'static,
{
    let (tx, rx) = futures::channel::oneshot::channel();
    buffer.map_async(
        MapMode::Read,
        bounds.clone(),
        on_buffer_mapped(buffer.clone(), bounds, tx),
    );
    AsyncBufferReadback { rx }
}

fn on_buffer_mapped<T, S>(
    buffer: Buffer,
    bounds: S,
    tx: Sender<anyhow::Result<T>>,
) -> impl FnOnce(Result<(), BufferAsyncError>)
where
    T: ShaderType + CreateFrom + Send + 'static,
    S: RangeBounds<BufferAddress> + Send + 'static,
{
    move |r| {
        if let Err(e) = r {
            tx.send(Err(e.into())).ok();
            dbg!();
            return;
        }

        let bytes = buffer.get_mapped_range(bounds).to_vec();
        buffer.unmap();
        let mut wrapper = encase::DynamicStorageBuffer::new(bytes);
        let data = wrapper.create::<T>();
        tx.send(data.map_err(Into::into)).ok();
    }
}
