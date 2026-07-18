use cyancia_utils::wrapper;

use crate::{
    blend_modes::BlendMode,
    composite::{BlendFunction, BlendFunctionId},
    layer::properties::{LayerProperties, LayerProperty},
};

wrapper! {
    #[derive(Debug, Clone, Copy)]
    pub VisibleProp : bool
}
impl LayerProperty for VisibleProp {}
impl Default for VisibleProp {
    fn default() -> Self {
        Self(true)
    }
}
pub trait VisiblePropertyExt {
    fn visible(&self) -> bool {
        self.get_visible().unwrap()
    }
    fn get_visible(&self) -> Option<bool>;
    fn set_visible(&mut self, visible: bool);
}
impl VisiblePropertyExt for LayerProperties {
    fn get_visible(&self) -> Option<bool> {
        Some(self.get::<VisibleProp>()?.0)
    }

    fn set_visible(&mut self, visible: bool) {
        self.set::<VisibleProp>(VisibleProp(visible));
    }
}

wrapper! {
    #[derive(Debug, Clone)]
    pub BlendFunctionProp : BlendFunctionId
}
impl LayerProperty for BlendFunctionProp {}
impl Default for BlendFunctionProp {
    fn default() -> Self {
        Self(BlendMode::Normal.id())
    }
}
pub trait BlendFunctionPropertyExt {
    fn blend_function(&self) -> &BlendFunctionId {
        self.get_blend_function().unwrap()
    }
    fn get_blend_function(&self) -> Option<&BlendFunctionId>;
    fn set_blend_function(&mut self, blend_function: BlendFunctionId);
}
impl BlendFunctionPropertyExt for LayerProperties {
    fn get_blend_function(&self) -> Option<&BlendFunctionId> {
        Some(&self.get::<BlendFunctionProp>()?.0)
    }

    fn set_blend_function(&mut self, blend_function: BlendFunctionId) {
        self.set::<BlendFunctionProp>(BlendFunctionProp(blend_function));
    }
}

wrapper! {
    #[derive(Debug, Clone, Copy)]
    pub OpacityProp : f32
}
impl LayerProperty for OpacityProp {}
impl Default for OpacityProp {
    fn default() -> Self {
        Self(1.0)
    }
}
pub trait OpacityPropertyExt {
    fn opacity(&self) -> f32 {
        self.get_opacity().unwrap()
    }
    fn get_opacity(&self) -> Option<f32>;
    fn set_opacity(&mut self, opacity: f32);
}
impl OpacityPropertyExt for LayerProperties {
    fn get_opacity(&self) -> Option<f32> {
        Some(self.get::<OpacityProp>()?.0)
    }

    fn set_opacity(&mut self, opacity: f32) {
        self.set::<OpacityProp>(OpacityProp(opacity));
    }
}

wrapper! {
    #[derive(Debug, Clone, Default)]
    pub NameProp : String
}
impl LayerProperty for NameProp {}
pub trait NamePropertyExt {
    fn name(&self) -> &str {
        self.get_name().unwrap()
    }
    fn get_name(&self) -> Option<&str>;
    fn set_name(&mut self, name: String);
}
impl NamePropertyExt for LayerProperties {
    fn get_name(&self) -> Option<&str> {
        Some(&self.get::<NameProp>()?.0)
    }

    fn set_name(&mut self, name: String) {
        self.set::<NameProp>(NameProp(name));
    }
}

wrapper! {
    #[derive(Debug, Clone, Copy, Default)]
    pub LockedProp : bool
}
impl LayerProperty for LockedProp {}
pub trait LockedPropertyExt {
    fn locked(&self) -> bool {
        self.get_locked().unwrap()
    }
    fn get_locked(&self) -> Option<bool>;
    fn set_locked(&mut self, locked: bool);
}
impl LockedPropertyExt for LayerProperties {
    fn get_locked(&self) -> Option<bool> {
        Some(self.get::<LockedProp>()?.0)
    }

    fn set_locked(&mut self, locked: bool) {
        self.set::<LockedProp>(LockedProp(locked));
    }
}

wrapper! {
    #[derive(Debug, Clone, Copy, Default)]
    pub DisabledChannelsProp : u32
}
impl LayerProperty for DisabledChannelsProp {}
pub trait DisabledChannelsPropertyExt {
    fn disabled_channels(&self) -> DisabledChannelsProp {
        self.get_disabled_channels().unwrap()
    }
    fn get_disabled_channels(&self) -> Option<DisabledChannelsProp>;
    fn set_disabled_channels(&mut self, channels: DisabledChannelsProp);
}
impl DisabledChannelsPropertyExt for LayerProperties {
    fn get_disabled_channels(&self) -> Option<DisabledChannelsProp> {
        Some(*self.get::<DisabledChannelsProp>()?)
    }

    fn set_disabled_channels(&mut self, channels: DisabledChannelsProp) {
        self.set(channels);
    }
}
impl DisabledChannelsProp {
    pub fn is_channel_disabled(&self, channel: u32) -> bool {
        (self.0 & (1 << channel)) != 0
    }

    pub fn set_channel_disabled(&mut self, channel: u32, disabled: bool) {
        if disabled {
            self.0 |= 1 << channel;
        } else {
            self.0 &= !(1 << channel);
        }
    }

    pub fn toggle_channel_disabled(&mut self, channel: u32) {
        self.set_channel_disabled(channel, !self.is_channel_disabled(channel));
    }
}

wrapper! {
    #[derive(Debug, Clone, Copy, Default)]
    pub LockedChannelsProp : u32
}
impl LayerProperty for LockedChannelsProp {}
pub trait LockedChannelsPropertyExt {
    fn locked_channels(&self) -> LockedChannelsProp {
        self.get_locked_channels().unwrap()
    }
    fn get_locked_channels(&self) -> Option<LockedChannelsProp>;
    fn set_locked_channels(&mut self, channels: LockedChannelsProp);
}
impl LockedChannelsPropertyExt for LayerProperties {
    fn get_locked_channels(&self) -> Option<LockedChannelsProp> {
        Some(*self.get::<LockedChannelsProp>()?)
    }

    fn set_locked_channels(&mut self, channels: LockedChannelsProp) {
        self.set(channels);
    }
}
impl LockedChannelsProp {
    pub fn is_channel_locked(&self, channel: u32) -> bool {
        (self.0 & (1 << channel)) != 0
    }

    pub fn set_channel_locked(&mut self, channel: u32, locked: bool) {
        if locked {
            self.0 |= 1 << channel;
        } else {
            self.0 &= !(1 << channel);
        }
    }

    pub fn toggle_channel_locked(&mut self, channel: u32) {
        self.set_channel_locked(channel, !self.is_channel_locked(channel));
    }
}
