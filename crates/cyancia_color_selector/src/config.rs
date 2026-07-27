use std::f32::consts::TAU;

use cyancia_color::model::rgb::Rgb;
use cyancia_widgets::spin_slider::{SpinSlider, SpinSliderEvent, SpinSliderState};
use gpui::{
    AnyElement, AppContext, Context, Entity, EventEmitter, Hsla, InteractiveElement, IntoElement,
    ParentElement, Render, Rgba, SharedString, Styled, Window, div, px,
};
use gpui_component::{
    ActiveTheme, Disableable, IndexPath, Sizable,
    button::{Button, ButtonVariants},
    checkbox::Checkbox,
    color_picker::{ColorPicker, ColorPickerEvent, ColorPickerState},
    h_flex,
    input::{Input, InputEvent, InputState},
    radio::RadioGroup,
    scroll::ScrollableElement,
    searchable_list::SearchableListItem,
    select::{SearchableVec, Select, SelectEvent, SelectState},
    v_flex,
};

use crate::{ColorModel, GradientPlaneShape};

#[derive(Debug, Clone)]
pub struct ColorSelectorConfig {
    pub name: String,
    pub max_plane_size: u32,
    pub max_planes_per_row: usize,
    pub planes: Vec<GradientPlaneConfig>,
    pub bars: Vec<GradientBarConfig>,
    pub out_of_gamut_color: Option<Rgb>,
    pub clip_to_gamut: bool,
}

#[derive(Debug, Clone)]
pub struct GradientPlaneConfig {
    pub model: ColorModel,
    pub shape: GradientPlaneShape,
    pub variable_channels: u8,
    pub flip_axis: GradientPlaneFlipAxis,
    pub rotation: f32,
    pub show_primary_channel_ring: bool,
    pub primary_channel_ring_width: f32,
    pub saturated_primary_channel: bool,
    pub ring_rotation: f32,
    pub reversed_ring: bool,
}

bitflags::bitflags! {
    #[derive(Debug, Clone, Copy)]
    pub struct GradientPlaneFlipAxis : u8 {
        const X = 0b01;
        const Y = 0b10;
    }
}

#[derive(Debug, Clone)]
pub struct GradientBarConfig {
    pub model: ColorModel,
    pub channel: u8,
    pub bar_height: f32,
    pub show_channel_label: bool,
    pub show_precise_spin_box: bool,
    pub show_primary_channel_lock: bool,
}

impl SearchableListItem for ColorModel {
    type Value = Self;

    fn title(&self) -> SharedString {
        self.to_string().into()
    }

    fn value(&self) -> &Self::Value {
        self
    }
}

impl SearchableListItem for GradientPlaneShape {
    type Value = Self;

    fn title(&self) -> SharedString {
        self.to_string().into()
    }

    fn value(&self) -> &Self::Value {
        self
    }
}

#[derive(Clone)]
struct ConfigItem {
    index: usize,
    name: SharedString,
}

impl SearchableListItem for ConfigItem {
    type Value = usize;

    fn title(&self) -> SharedString {
        self.name.clone()
    }

    fn value(&self) -> &Self::Value {
        &self.index
    }
}

type ConfigSelectState = SelectState<SearchableVec<ConfigItem>>;
type ColorModelSelectState = SelectState<SearchableVec<ColorModel>>;
type PlaneShapeSelectState = SelectState<SearchableVec<GradientPlaneShape>>;

#[derive(Clone)]
struct PlaneEditorControls {
    id: u64,
    model: Entity<ColorModelSelectState>,
    shape: Entity<PlaneShapeSelectState>,
    rotation: Entity<SpinSliderState>,
    ring_width: Entity<SpinSliderState>,
    ring_rotation: Entity<SpinSliderState>,
}

#[derive(Clone)]
struct BarEditorControls {
    id: u64,
    model: Entity<ColorModelSelectState>,
    bar_height: Entity<SpinSliderState>,
}

pub struct ColorSelectorConfigEditorState {
    configs: Vec<ColorSelectorConfig>,
    selected_config: Option<usize>,
    config_select: Entity<ConfigSelectState>,
    name: Entity<InputState>,
    max_plane_size: Entity<SpinSliderState>,
    max_planes_per_row: Entity<SpinSliderState>,
    out_of_gamut_color: Entity<ColorPickerState>,
    plane_controls: Vec<PlaneEditorControls>,
    bar_controls: Vec<BarEditorControls>,
    next_control_id: u64,
}

impl ColorSelectorConfigEditorState {
    pub fn new(
        configs: Vec<ColorSelectorConfig>,
        selected_config: Option<usize>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let selected_config = Self::valid_selection(&configs, selected_config);
        let config_select = Self::create_config_select(&configs, selected_config, window, cx);
        let name_value = selected_config
            .and_then(|index| configs.get(index))
            .map_or("", |config| config.name.as_str());
        let name = Self::create_name_input(name_value, window, cx);
        let active_config = selected_config.and_then(|index| configs.get(index));
        let (max_plane_size, max_planes_per_row) =
            Self::create_config_layout_controls(active_config, window, cx);
        let out_of_gamut_color = Self::create_out_of_gamut_color_control(active_config, window, cx);

        let mut this = Self {
            configs,
            selected_config,
            config_select,
            name,
            max_plane_size,
            max_planes_per_row,
            out_of_gamut_color,
            plane_controls: Vec::new(),
            bar_controls: Vec::new(),
            next_control_id: 0,
        };

        this.subscribe_config_select(window, cx);
        this.subscribe_name(window, cx);
        this.subscribe_config_layout_controls(window, cx);
        this.subscribe_out_of_gamut_color(window, cx);
        this.rebuild_active_controls(window, cx);
        this
    }

    pub fn configs(&self) -> &[ColorSelectorConfig] {
        &self.configs
    }

    pub fn reset(
        &mut self,
        configs: Vec<ColorSelectorConfig>,
        selected_config: Option<usize>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.configs = configs;
        self.selected_config = Self::valid_selection(&self.configs, selected_config);
        self.refresh_config_select(window, cx);
        self.rebuild_active_editor(window, cx);
        cx.notify();
    }

    fn valid_selection(
        configs: &[ColorSelectorConfig],
        selected_config: Option<usize>,
    ) -> Option<usize> {
        if configs.is_empty() {
            None
        } else {
            Some(selected_config.unwrap_or(0).min(configs.len() - 1))
        }
    }

    fn config_items(configs: &[ColorSelectorConfig]) -> SearchableVec<ConfigItem> {
        SearchableVec::new(
            configs
                .iter()
                .enumerate()
                .map(|(index, config)| ConfigItem {
                    index,
                    name: config.name.clone().into(),
                })
                .collect::<Vec<_>>(),
        )
    }

    fn create_config_select(
        configs: &[ColorSelectorConfig],
        selected_config: Option<usize>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<ConfigSelectState> {
        let items = Self::config_items(configs);
        cx.new(|cx| {
            SelectState::new(items, selected_config.map(IndexPath::new), window, cx)
                .searchable(false)
        })
    }

    fn create_name_input(
        value: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<InputState> {
        cx.new(|cx| {
            InputState::new(window, cx)
                .default_value(value.to_owned())
                .placeholder("Config name")
        })
    }

    fn create_config_layout_controls(
        config: Option<&ColorSelectorConfig>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> (Entity<SpinSliderState>, Entity<SpinSliderState>) {
        let max_plane_size = cx.new(|cx| {
            SpinSliderState::new(128.0, 512.0, window, cx)
                .precision(0, window, cx)
                .value(
                    config.map_or(512.0, |config| config.max_plane_size as f32),
                    window,
                    cx,
                )
        });
        let max_planes_per_row = cx.new(|cx| {
            SpinSliderState::new(1.0, 5.0, window, cx)
                .precision(0, window, cx)
                .value(
                    config.map_or(2.0, |config| config.max_planes_per_row as f32),
                    window,
                    cx,
                )
        });
        (max_plane_size, max_planes_per_row)
    }

    fn create_out_of_gamut_color_control(
        config: Option<&ColorSelectorConfig>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<ColorPickerState> {
        let color = config
            .and_then(|config| config.out_of_gamut_color)
            .unwrap_or(Rgb::new(0.5, 0.5, 0.5));
        cx.new(|cx| {
            ColorPickerState::new(window, cx).default_value(Rgba {
                r: color.r,
                g: color.g,
                b: color.b,
                a: 1.0,
            })
        })
    }

    fn subscribe_config_select(&self, window: &mut Window, cx: &mut Context<Self>) {
        cx.subscribe_in(&self.config_select, window, |this, _, event, window, cx| {
            if let SelectEvent::Confirm(Some(index)) = event {
                this.selected_config = Some(*index);
                this.rebuild_active_editor(window, cx);
                cx.notify();
            }
        })
        .detach();
    }

    fn subscribe_name(&self, window: &mut Window, cx: &mut Context<Self>) {
        cx.subscribe_in(&self.name, window, |this, input, event, window, cx| {
            if !matches!(event, InputEvent::Change) {
                return;
            }

            let value = input.read(cx).value().to_string();
            if let Some(config) = this.active_config_mut() {
                config.name = value;
                this.refresh_config_select(window, cx);
                cx.notify();
            }
        })
        .detach();
    }

    fn subscribe_config_layout_controls(&self, window: &mut Window, cx: &mut Context<Self>) {
        cx.subscribe_in(&self.max_plane_size, window, |this, _, event, _, cx| {
            if let SpinSliderEvent::Change(value) = event
                && let Some(config) = this.active_config_mut()
            {
                config.max_plane_size = value.round() as u32;
                cx.notify();
            }
        })
        .detach();
        cx.subscribe_in(&self.max_planes_per_row, window, |this, _, event, _, cx| {
            if let SpinSliderEvent::Change(value) = event
                && let Some(config) = this.active_config_mut()
            {
                config.max_planes_per_row = value.round() as usize;
                cx.notify();
            }
        })
        .detach();
    }

    fn subscribe_out_of_gamut_color(&self, window: &mut Window, cx: &mut Context<Self>) {
        cx.subscribe_in(&self.out_of_gamut_color, window, |this, _, event, _, cx| {
            let ColorPickerEvent::Change(color) = event;
            if let Some(config) = this.active_config_mut() {
                config.out_of_gamut_color = color.map(|c| {
                    let color = Rgba::from(c);
                    Rgb::new(color.r, color.g, color.b)
                });
                cx.notify();
            }
        })
        .detach();
    }

    fn refresh_config_select(&self, window: &mut Window, cx: &mut Context<Self>) {
        let items = Self::config_items(&self.configs);
        self.config_select.update(cx, |state, cx| {
            state.set_items(items, window, cx);
            state.set_selected_index(self.selected_config.map(IndexPath::new), window, cx);
        });
    }

    fn rebuild_active_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let name = self
            .active_config()
            .map_or("", |config| config.name.as_str());
        self.name = Self::create_name_input(name, window, cx);
        let (max_plane_size, max_planes_per_row) =
            Self::create_config_layout_controls(self.active_config(), window, cx);
        self.max_plane_size = max_plane_size;
        self.max_planes_per_row = max_planes_per_row;
        self.out_of_gamut_color =
            Self::create_out_of_gamut_color_control(self.active_config(), window, cx);
        self.subscribe_name(window, cx);
        self.subscribe_config_layout_controls(window, cx);
        self.subscribe_out_of_gamut_color(window, cx);
        self.rebuild_active_controls(window, cx);
    }

    fn rebuild_active_controls(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.plane_controls.clear();
        self.bar_controls.clear();

        let Some(config) = self.active_config().cloned() else {
            return;
        };
        for plane in &config.planes {
            let controls = self.create_plane_controls(plane, window, cx);
            self.plane_controls.push(controls);
        }
        for bar in &config.bars {
            let controls = self.create_bar_controls(bar, window, cx);
            self.bar_controls.push(controls);
        }
    }

    fn active_config(&self) -> Option<&ColorSelectorConfig> {
        self.selected_config
            .and_then(|index| self.configs.get(index))
    }

    fn active_config_mut(&mut self) -> Option<&mut ColorSelectorConfig> {
        self.selected_config
            .and_then(|index| self.configs.get_mut(index))
    }

    fn create_model_select(
        models: &[ColorModel],
        selected: ColorModel,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<ColorModelSelectState> {
        let items = models.to_vec();
        let selected = models.iter().position(|model| *model == selected);
        cx.new(|cx| {
            SelectState::new(
                SearchableVec::new(items),
                selected.map(IndexPath::new),
                window,
                cx,
            )
        })
    }

    fn create_plane_controls(
        &mut self,
        config: &GradientPlaneConfig,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> PlaneEditorControls {
        let id = self.next_control_id;
        self.next_control_id += 1;
        let model = Self::create_model_select(&ColorModel::PLANE_MODELS, config.model, window, cx);
        let shapes = [GradientPlaneShape::Square, GradientPlaneShape::Triangle];
        let shape_items = shapes.to_vec();
        let selected_shape = shapes.iter().position(|shape| *shape == config.shape);
        let shape = cx.new(|cx| {
            SelectState::new(
                SearchableVec::new(shape_items),
                selected_shape.map(IndexPath::new),
                window,
                cx,
            )
        });
        let rotation = cx.new(|cx| {
            SpinSliderState::new(0.0, TAU, window, cx)
                .precision(3, window, cx)
                .value(config.rotation.rem_euclid(TAU), window, cx)
        });
        let ring_width = cx.new(|cx| {
            SpinSliderState::new(10.0, 40.0, window, cx)
                .precision(1, window, cx)
                .value(config.primary_channel_ring_width, window, cx)
        });
        let ring_rotation = cx.new(|cx| {
            SpinSliderState::new(0.0, TAU, window, cx)
                .precision(3, window, cx)
                .value(config.ring_rotation.rem_euclid(TAU), window, cx)
        });

        cx.subscribe_in(&model, window, move |this, _, event, _, cx| {
            if let SelectEvent::Confirm(Some(model)) = event
                && let Some(index) = this.plane_index(id)
            {
                this.active_config_mut().unwrap().planes[index].model = *model;
                cx.notify();
            }
        })
        .detach();
        cx.subscribe_in(&shape, window, move |this, _, event, _, cx| {
            if let SelectEvent::Confirm(Some(shape)) = event
                && let Some(index) = this.plane_index(id)
            {
                this.active_config_mut().unwrap().planes[index].shape = *shape;
                cx.notify();
            }
        })
        .detach();
        cx.subscribe_in(&rotation, window, move |this, _, event, _, cx| {
            if let SpinSliderEvent::Change(rotation) = event
                && let Some(index) = this.plane_index(id)
            {
                this.active_config_mut().unwrap().planes[index].rotation = *rotation;
                cx.notify();
            }
        })
        .detach();
        cx.subscribe_in(&ring_width, window, move |this, _, event, _, cx| {
            if let SpinSliderEvent::Change(width) = event
                && let Some(index) = this.plane_index(id)
            {
                this.active_config_mut().unwrap().planes[index].primary_channel_ring_width = *width;
                cx.notify();
            }
        })
        .detach();
        cx.subscribe_in(&ring_rotation, window, move |this, _, event, _, cx| {
            if let SpinSliderEvent::Change(rotation) = event
                && let Some(index) = this.plane_index(id)
            {
                this.active_config_mut().unwrap().planes[index].ring_rotation = *rotation;
                cx.notify();
            }
        })
        .detach();

        PlaneEditorControls {
            id,
            model,
            shape,
            rotation,
            ring_width,
            ring_rotation,
        }
    }

    fn create_bar_controls(
        &mut self,
        config: &GradientBarConfig,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> BarEditorControls {
        let id = self.next_control_id;
        self.next_control_id += 1;
        let model = Self::create_model_select(&ColorModel::ALL, config.model, window, cx);
        let bar_height = cx.new(|cx| {
            SpinSliderState::new(10.0, 40.0, window, cx)
                .precision(1, window, cx)
                .value(config.bar_height, window, cx)
        });

        cx.subscribe_in(&model, window, move |this, _, event, _, cx| {
            if let SelectEvent::Confirm(Some(model)) = event
                && let Some(index) = this.bar_index(id)
            {
                let bar = &mut this.active_config_mut().unwrap().bars[index];
                bar.model = *model;
                bar.channel = bar
                    .channel
                    .min(model.channel_labels().len().saturating_sub(1) as u8);
                cx.notify();
            }
        })
        .detach();
        cx.subscribe_in(&bar_height, window, move |this, _, event, _, cx| {
            if let SpinSliderEvent::Change(height) = event
                && let Some(index) = this.bar_index(id)
            {
                this.active_config_mut().unwrap().bars[index].bar_height = *height;
                cx.notify();
            }
        })
        .detach();

        BarEditorControls {
            id,
            model,
            bar_height,
        }
    }

    fn plane_index(&self, id: u64) -> Option<usize> {
        self.plane_controls
            .iter()
            .position(|controls| controls.id == id)
    }

    fn bar_index(&self, id: u64) -> Option<usize> {
        self.bar_controls
            .iter()
            .position(|controls| controls.id == id)
    }

    fn add_plane(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let config = GradientPlaneConfig {
            model: ColorModel::Hsv,
            shape: GradientPlaneShape::Square,
            variable_channels: 0b110,
            flip_axis: GradientPlaneFlipAxis::empty(),
            rotation: 0.0,
            show_primary_channel_ring: false,
            primary_channel_ring_width: 20.0,
            saturated_primary_channel: false,
            ring_rotation: 0.0,
            reversed_ring: false,
        };
        let controls = self.create_plane_controls(&config, window, cx);
        self.active_config_mut().unwrap().planes.push(config);
        self.plane_controls.push(controls);
        cx.notify();
    }

    fn remove_plane(&mut self, index: usize, cx: &mut Context<Self>) {
        self.active_config_mut().unwrap().planes.remove(index);
        self.plane_controls.remove(index);
        cx.notify();
    }

    fn move_plane(&mut self, index: usize, offset: isize, cx: &mut Context<Self>) {
        let target = index.saturating_add_signed(offset);
        if target >= self.active_config().unwrap().planes.len() {
            return;
        }
        self.active_config_mut().unwrap().planes.swap(index, target);
        self.plane_controls.swap(index, target);
        cx.notify();
    }

    fn add_bar(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let config = GradientBarConfig {
            model: ColorModel::Rgb,
            channel: 0,
            bar_height: 20.0,
            show_channel_label: true,
            show_precise_spin_box: true,
            show_primary_channel_lock: false,
        };
        let controls = self.create_bar_controls(&config, window, cx);
        self.active_config_mut().unwrap().bars.push(config);
        self.bar_controls.push(controls);
        cx.notify();
    }

    fn remove_bar(&mut self, index: usize, cx: &mut Context<Self>) {
        self.active_config_mut().unwrap().bars.remove(index);
        self.bar_controls.remove(index);
        cx.notify();
    }

    fn move_bar(&mut self, index: usize, offset: isize, cx: &mut Context<Self>) {
        let target = index.saturating_add_signed(offset);
        if target >= self.active_config().unwrap().bars.len() {
            return;
        }
        self.active_config_mut().unwrap().bars.swap(index, target);
        self.bar_controls.swap(index, target);
        cx.notify();
    }

    fn add_config(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.configs.push(ColorSelectorConfig {
            name: format!("Config {}", self.configs.len() + 1),
            max_plane_size: 512,
            max_planes_per_row: 2,
            planes: Vec::new(),
            bars: Vec::new(),
            out_of_gamut_color: None,
            clip_to_gamut: false,
        });
        self.selected_config = Some(self.configs.len() - 1);
        self.refresh_config_select(window, cx);
        self.rebuild_active_editor(window, cx);
        cx.notify();
    }

    fn remove_config(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(index) = self.selected_config else {
            return;
        };

        self.configs.remove(index);
        self.selected_config = if self.configs.is_empty() {
            None
        } else {
            Some(index.min(self.configs.len() - 1))
        };
        self.refresh_config_select(window, cx);
        self.rebuild_active_editor(window, cx);
        cx.notify();
    }

    fn move_config(&mut self, offset: isize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(index) = self.selected_config else {
            return;
        };
        let target = index.saturating_add_signed(offset);
        if target >= self.configs.len() || target == index {
            return;
        }

        self.configs.swap(index, target);
        self.selected_config = Some(target);
        self.refresh_config_select(window, cx);
        cx.notify();
    }

    fn render_plane(&self, index: usize, cx: &mut Context<Self>) -> AnyElement {
        let config = self.active_config().unwrap().planes[index].clone();
        let controls = self.plane_controls[index].clone();
        let labels = config.model.channel_labels();
        let primary_channel = (0..3)
            .find(|channel| config.variable_channels & (1u8 << channel) == 0)
            .unwrap_or(0);
        let id = controls.id;

        v_flex()
            .gap_2()
            .p_3()
            .border_1()
            .border_color(cx.theme().border)
            .rounded(cx.theme().radius)
            .child(
                h_flex()
                    .justify_between()
                    .child(format!("Plane {}", index + 1))
                    .child(
                        h_flex()
                            .gap_1()
                            .child(
                                Button::new(format!("plane-up-{id}"))
                                    .xsmall()
                                    .label("Up")
                                    .disabled(index == 0)
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.move_plane(index, -1, cx)
                                    })),
                            )
                            .child(
                                Button::new(format!("plane-down-{id}"))
                                    .xsmall()
                                    .label("Down")
                                    .disabled(
                                        index + 1 == self.active_config().unwrap().planes.len(),
                                    )
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.move_plane(index, 1, cx)
                                    })),
                            )
                            .child(
                                Button::new(format!("plane-remove-{id}"))
                                    .xsmall()
                                    .danger()
                                    .label("Remove")
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.remove_plane(index, cx)
                                    })),
                            ),
                    ),
            )
            .child(
                h_flex()
                    .gap_2()
                    .child(div().w(px(110.)).child("Model"))
                    .child(div().flex_1().child(Select::new(&controls.model).small())),
            )
            .child(
                h_flex()
                    .gap_2()
                    .child(div().w(px(110.)).child("Shape"))
                    .child(div().flex_1().child(Select::new(&controls.shape).small())),
            )
            .child(
                h_flex()
                    .gap_2()
                    .child(div().w(px(110.)).child("Primary channel"))
                    .child(
                        RadioGroup::horizontal(format!("plane-primary-{id}"))
                            .children(labels.iter().copied())
                            .selected_index(Some(primary_channel))
                            .on_click(cx.listener(move |this, channel: &usize, _, cx| {
                                if let Some(index) = this.plane_index(id) {
                                    this.active_config_mut().unwrap().planes[index]
                                        .variable_channels = 0b111u8 & !(1u8 << *channel);
                                    cx.notify();
                                }
                            })),
                    ),
            )
            .child(
                h_flex()
                    .gap_4()
                    .child(
                        Checkbox::new(format!("plane-flip-x-{id}"))
                            .label("Flip X")
                            .checked(config.flip_axis.contains(GradientPlaneFlipAxis::X))
                            .on_click(cx.listener(move |this, checked, _, cx| {
                                if let Some(index) = this.plane_index(id) {
                                    this.active_config_mut().unwrap().planes[index]
                                        .flip_axis
                                        .set(GradientPlaneFlipAxis::X, *checked);
                                    cx.notify();
                                }
                            })),
                    )
                    .child(
                        Checkbox::new(format!("plane-flip-y-{id}"))
                            .label("Flip Y")
                            .checked(config.flip_axis.contains(GradientPlaneFlipAxis::Y))
                            .on_click(cx.listener(move |this, checked, _, cx| {
                                if let Some(index) = this.plane_index(id) {
                                    this.active_config_mut().unwrap().planes[index]
                                        .flip_axis
                                        .set(GradientPlaneFlipAxis::Y, *checked);
                                    cx.notify();
                                }
                            })),
                    ),
            )
            .child(
                SpinSlider::new(&controls.rotation)
                    .small()
                    .prefix("Rotation: ")
                    .suffix(" rad"),
            )
            .child(
                h_flex()
                    .gap_4()
                    .child(
                        Checkbox::new(format!("plane-show-ring-{id}"))
                            .label("Primary channel ring")
                            .checked(config.show_primary_channel_ring)
                            .on_click(cx.listener(move |this, checked, _, cx| {
                                if let Some(index) = this.plane_index(id) {
                                    this.active_config_mut().unwrap().planes[index]
                                        .show_primary_channel_ring = *checked;
                                    cx.notify();
                                }
                            })),
                    )
                    .child(
                        Checkbox::new(format!("plane-saturated-primary-channel-{id}"))
                            .label("Saturated primary channel")
                            .checked(config.saturated_primary_channel)
                            .on_click(cx.listener(move |this, checked, _, cx| {
                                if let Some(index) = this.plane_index(id) {
                                    this.active_config_mut().unwrap().planes[index]
                                        .saturated_primary_channel = *checked;
                                    cx.notify();
                                }
                            })),
                    )
                    .child(
                        Checkbox::new(format!("plane-reversed-ring-{id}"))
                            .label("Reversed ring")
                            .checked(config.reversed_ring)
                            .disabled(!config.show_primary_channel_ring)
                            .on_click(cx.listener(move |this, checked, _, cx| {
                                if let Some(index) = this.plane_index(id) {
                                    this.active_config_mut().unwrap().planes[index].reversed_ring =
                                        *checked;
                                    cx.notify();
                                }
                            })),
                    ),
            )
            .child(
                SpinSlider::new(&controls.ring_width)
                    .small()
                    .disabled(!config.show_primary_channel_ring)
                    .prefix("Ring width: ")
                    .suffix(" px"),
            )
            .child(
                SpinSlider::new(&controls.ring_rotation)
                    .small()
                    .disabled(!config.show_primary_channel_ring)
                    .prefix("Ring rotation: ")
                    .suffix(" rad"),
            )
            .into_any_element()
    }

    fn render_bar(&self, index: usize, cx: &mut Context<Self>) -> AnyElement {
        let config = self.active_config().unwrap().bars[index].clone();
        let controls = self.bar_controls[index].clone();
        let labels = config.model.channel_labels();
        let id = controls.id;

        v_flex()
            .gap_2()
            .p_3()
            .border_1()
            .border_color(cx.theme().border)
            .rounded(cx.theme().radius)
            .child(
                h_flex()
                    .justify_between()
                    .child(format!("Bar {}", index + 1))
                    .child(
                        h_flex()
                            .gap_1()
                            .child(
                                Button::new(format!("bar-up-{id}"))
                                    .xsmall()
                                    .label("Up")
                                    .disabled(index == 0)
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.move_bar(index, -1, cx)
                                    })),
                            )
                            .child(
                                Button::new(format!("bar-down-{id}"))
                                    .xsmall()
                                    .label("Down")
                                    .disabled(index + 1 == self.active_config().unwrap().bars.len())
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.move_bar(index, 1, cx)
                                    })),
                            )
                            .child(
                                Button::new(format!("bar-remove-{id}"))
                                    .xsmall()
                                    .danger()
                                    .label("Remove")
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.remove_bar(index, cx)
                                    })),
                            ),
                    ),
            )
            .child(
                h_flex()
                    .gap_2()
                    .child(div().w(px(110.)).child("Model"))
                    .child(div().flex_1().child(Select::new(&controls.model).small())),
            )
            .child(
                SpinSlider::new(&controls.bar_height)
                    .small()
                    .prefix("Bar height: ")
                    .suffix(" px"),
            )
            .child(
                h_flex()
                    .gap_2()
                    .child(div().w(px(110.)).child("Channel"))
                    .child(
                        RadioGroup::horizontal(format!("bar-channel-{id}"))
                            .children(labels.iter().copied())
                            .selected_index(Some(config.channel as usize))
                            .on_click(cx.listener(move |this, channel: &usize, _, cx| {
                                if let Some(index) = this.bar_index(id) {
                                    this.active_config_mut().unwrap().bars[index].channel =
                                        *channel as u8;
                                    cx.notify();
                                }
                            })),
                    ),
            )
            .child(
                h_flex()
                    .gap_4()
                    .child(
                        Checkbox::new(format!("bar-label-{id}"))
                            .label("Channel label")
                            .checked(config.show_channel_label)
                            .on_click(cx.listener(move |this, checked, _, cx| {
                                if let Some(index) = this.bar_index(id) {
                                    this.active_config_mut().unwrap().bars[index]
                                        .show_channel_label = *checked;
                                    cx.notify();
                                }
                            })),
                    )
                    .child(
                        Checkbox::new(format!("bar-spin-{id}"))
                            .label("Precise spin box")
                            .checked(config.show_precise_spin_box)
                            .on_click(cx.listener(move |this, checked, _, cx| {
                                if let Some(index) = this.bar_index(id) {
                                    this.active_config_mut().unwrap().bars[index]
                                        .show_precise_spin_box = *checked;
                                    cx.notify();
                                }
                            })),
                    )
                    .child(
                        Checkbox::new(format!("bar-lock-{id}"))
                            .label("Primary channel lock")
                            .checked(config.show_primary_channel_lock)
                            .on_click(cx.listener(move |this, checked, _, cx| {
                                if let Some(index) = this.bar_index(id) {
                                    this.active_config_mut().unwrap().bars[index]
                                        .show_primary_channel_lock = *checked;
                                    cx.notify();
                                }
                            })),
                    ),
            )
            .into_any_element()
    }
}

impl Render for ColorSelectorConfigEditorState {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let selected = self.selected_config;
        let active_content = if let Some(config) = self.active_config() {
            let planes = (0..config.planes.len())
                .map(|index| self.render_plane(index, cx))
                .collect::<Vec<_>>();
            let bars = (0..config.bars.len())
                .map(|index| self.render_bar(index, cx))
                .collect::<Vec<_>>();

            v_flex()
                .gap_3()
                .child(
                    h_flex()
                        .gap_2()
                        .child(div().w(px(110.)).child("Name"))
                        .child(div().flex_1().child(Input::new(&self.name).small())),
                )
                .child(
                    SpinSlider::new(&self.max_plane_size)
                        .small()
                        .prefix("Max plane size: ")
                        .suffix(" px"),
                )
                .child(
                    SpinSlider::new(&self.max_planes_per_row)
                        .small()
                        .prefix("Max planes per row: "),
                )
                .child(
                    h_flex()
                        .gap_4()
                        .child(
                            Checkbox::new("color-selector-out-of-gamut-color")
                                .label("Out-of-gamut color")
                                .checked(config.out_of_gamut_color.is_some())
                                .on_click(cx.listener(|this, checked: &bool, _, cx| {
                                    let color = (*checked).then(|| {
                                        this.out_of_gamut_color
                                            .read(cx)
                                            .value()
                                            .map(|c| {
                                                let color = Rgba::from(c);
                                                Rgb::new(color.r, color.g, color.b)
                                            })
                                            .unwrap_or(Rgb::new(0.5, 0.5, 0.5))
                                    });
                                    this.active_config_mut().unwrap().out_of_gamut_color = color;
                                    cx.notify();
                                })),
                        )
                        .child(ColorPicker::new(&self.out_of_gamut_color).small())
                        .child(
                            Checkbox::new("color-selector-clip-to-gamut")
                                .label("Clip to gamut")
                                .checked(config.clip_to_gamut)
                                .on_click(cx.listener(|this, checked, _, cx| {
                                    this.active_config_mut().unwrap().clip_to_gamut = *checked;
                                    cx.notify();
                                })),
                        ),
                )
                .child(
                    v_flex()
                        .gap_2()
                        .child(
                            h_flex().justify_between().child("Planes").child(
                                Button::new("add-color-selector-plane")
                                    .small()
                                    .label("Add plane")
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.add_plane(window, cx)
                                    })),
                            ),
                        )
                        .children(planes),
                )
                .child(
                    v_flex()
                        .gap_2()
                        .child(
                            h_flex().justify_between().child("Bars").child(
                                Button::new("add-color-selector-bar")
                                    .small()
                                    .label("Add bar")
                                    .on_click(
                                        cx.listener(|this, _, window, cx| this.add_bar(window, cx)),
                                    ),
                            ),
                        )
                        .children(bars),
                )
                .into_any_element()
        } else {
            div()
                .py_4()
                .text_color(cx.theme().muted_foreground)
                .child("No configs. Add one to continue editing.")
                .into_any_element()
        };

        let content = div().flex_1().min_h_0().overflow_hidden().child(
            v_flex()
                .id("color-selector-config-editor-content")
                .size_full()
                .gap_3()
                .p_3()
                .child(
                    h_flex()
                        .gap_2()
                        .child(div().w(px(110.)).child("Config"))
                        .child(
                            div().flex_1().child(
                                Select::new(&self.config_select)
                                    .small()
                                    .placeholder("No configs"),
                            ),
                        )
                        .child(
                            Button::new("add-color-selector-config")
                                .small()
                                .label("Add")
                                .on_click(
                                    cx.listener(|this, _, window, cx| this.add_config(window, cx)),
                                ),
                        )
                        .child(
                            Button::new("move-color-selector-config-up")
                                .small()
                                .label("Up")
                                .disabled(selected.is_none_or(|index| index == 0))
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.move_config(-1, window, cx)
                                })),
                        )
                        .child(
                            Button::new("move-color-selector-config-down")
                                .small()
                                .label("Down")
                                .disabled(
                                    selected.is_none_or(|index| index + 1 == self.configs.len()),
                                )
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.move_config(1, window, cx)
                                })),
                        )
                        .child(
                            Button::new("remove-color-selector-config")
                                .small()
                                .danger()
                                .label("Remove")
                                .disabled(selected.is_none())
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.remove_config(window, cx)
                                })),
                        ),
                )
                .child(active_content)
                .overflow_y_scrollbar(),
        );

        v_flex()
            .size_full()
            .min_h_0()
            .overflow_hidden()
            .child(content)
            .child(
                h_flex()
                    .flex_shrink_0()
                    .justify_end()
                    .gap_2()
                    .p_3()
                    .child(
                        Button::new("cancel-color-selector-config")
                            .label("Cancel")
                            .on_click(cx.listener(|_, _, _, cx| {
                                cx.emit(ColorSelectorConfigEvent::Cancel);
                            })),
                    )
                    .child(
                        Button::new("confirm-color-selector-config")
                            .primary()
                            .label("Confirm")
                            .on_click(cx.listener(|_, _, _, cx| {
                                cx.emit(ColorSelectorConfigEvent::Confirm);
                            })),
                    ),
            )
    }
}

#[derive(Debug, Clone, Copy)]
pub enum ColorSelectorConfigEvent {
    Confirm,
    Cancel,
}

impl EventEmitter<ColorSelectorConfigEvent> for ColorSelectorConfigEditorState {}
