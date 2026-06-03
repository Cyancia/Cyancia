use std::sync::Arc;

use cyancia_utils::include_shader;
use futures::executor::block_on;
use gpui::{App, Global};
use wgpu::{
    Adapter, AddressMode, Backends, Device, Features, FilterMode, Instance, Limits, Queue, Sampler,
    SamplerDescriptor, ShaderModule, ShaderModuleDescriptor, ShaderSource, VertexState,
};

use crate::render_context::RenderContext;

#[derive(Debug)]
pub struct GlobalSamplers {
    nearest_clamp: Arc<Sampler>,
    linear_clamp: Arc<Sampler>,
    nearest_wrap: Arc<Sampler>,
    linear_wrap: Arc<Sampler>,
}

impl Global for GlobalSamplers {}

impl GlobalSamplers {
    pub fn from_app(app: &App) -> Self {
        let render_context = app.global::<RenderContext>();
        Self::new(&render_context.device)
    }

    pub fn new(device: &Device) -> Self {
        let nearest_clamp = device.create_sampler(&SamplerDescriptor {
            label: Some("nearest clamp sampler"),
            address_mode_u: AddressMode::ClampToEdge,
            address_mode_v: AddressMode::ClampToEdge,
            address_mode_w: AddressMode::ClampToEdge,
            mag_filter: FilterMode::Nearest,
            min_filter: FilterMode::Nearest,
            mipmap_filter: FilterMode::Nearest,
            ..Default::default()
        });

        let linear_clamp = device.create_sampler(&SamplerDescriptor {
            label: Some("linear clamp sampler"),
            address_mode_u: AddressMode::ClampToEdge,
            address_mode_v: AddressMode::ClampToEdge,
            address_mode_w: AddressMode::ClampToEdge,
            mag_filter: FilterMode::Linear,
            min_filter: FilterMode::Linear,
            mipmap_filter: FilterMode::Linear,
            ..Default::default()
        });

        let nearest_wrap = device.create_sampler(&SamplerDescriptor {
            label: Some("nearest wrap sampler"),
            address_mode_u: AddressMode::Repeat,
            address_mode_v: AddressMode::Repeat,
            address_mode_w: AddressMode::Repeat,
            mag_filter: FilterMode::Nearest,
            min_filter: FilterMode::Nearest,
            mipmap_filter: FilterMode::Nearest,
            ..Default::default()
        });

        let linear_wrap = device.create_sampler(&SamplerDescriptor {
            label: Some("linear wrap sampler"),
            address_mode_u: AddressMode::Repeat,
            address_mode_v: AddressMode::Repeat,
            address_mode_w: AddressMode::Repeat,
            mag_filter: FilterMode::Linear,
            min_filter: FilterMode::Linear,
            mipmap_filter: FilterMode::Linear,
            ..Default::default()
        });

        Self {
            nearest_clamp: Arc::new(nearest_clamp),
            linear_clamp: Arc::new(linear_clamp),
            nearest_wrap: Arc::new(nearest_wrap),
            linear_wrap: Arc::new(linear_wrap),
        }
    }

    pub fn nearest_clamp(&self) -> &Sampler {
        &self.nearest_clamp
    }

    pub fn linear_clamp(&self) -> &Sampler {
        &self.linear_clamp
    }

    pub fn nearest_wrap(&self) -> &Sampler {
        &self.nearest_wrap
    }

    pub fn linear_wrap(&self) -> &Sampler {
        &self.linear_wrap
    }
}

#[derive(Debug)]
pub struct FullscreenVertex {
    shader: ShaderModule,
}

impl Global for FullscreenVertex {}

impl FullscreenVertex {
    pub fn from_app(cx: &App) -> Self {
        let render_context = cx.global::<RenderContext>();
        Self::new(&render_context.device)
    }

    pub fn new(device: &Device) -> Self {
        let fullscreen_vertex = device.create_shader_module(ShaderModuleDescriptor {
            label: Some("fullscreen vertex shader"),
            source: ShaderSource::Wgsl(include_str!("shaders/fullscreen_vertex.wgsl").into()),
        });

        Self {
            shader: fullscreen_vertex,
        }
    }

    pub fn fullscreen_vertex(&self) -> &ShaderModule {
        &self.shader
    }

    pub fn fullscreen_vertex_state(&self) -> VertexState<'_> {
        VertexState {
            module: &self.shader,
            entry_point: Some("vertex"),
            compilation_options: Default::default(),
            buffers: &[],
        }
    }
}
