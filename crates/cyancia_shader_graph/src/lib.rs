pub mod editor;
pub mod graph;
pub mod save;
pub mod wgsl_std;

pub type GraphTheme = iced_core::Theme;
pub type GraphRenderer = iced_wgpu::Renderer;
pub type GraphSerializer<'a> = toml::Serializer<'a>;
pub type GraphDeserializer<'a> = toml::de::Deserializer<'a>;
