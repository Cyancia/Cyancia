use std::f32::consts::TAU;

use cyancia_color::model::rgb::Rgb;
use cyancia_widgets::spin_slider::{SpinSlider, SpinSliderEvent, SpinSliderState};
use gpui::{
    AnyElement, AppContext, Context, Entity, EventEmitter, InteractiveElement, IntoElement,
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
    pub out_of_gamut_color: Rgb,
    pub use_out_of_gamut_color: bool,
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
    pub ring_bar_saturated_hue_channel: bool,
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
    model: Entity<ColorModelSelectState>,
    shape: Entity<PlaneShapeSelectState>,
    rotation: Entity<SpinSliderState>,
    ring_width: Entity<SpinSliderState>,
    ring_rotation: Entity<SpinSliderState>,
}

#[derive(Clone)]
struct BarEditorControls {
    model: Entity<ColorModelSelectState>,
    bar_height: Entity<SpinSliderState>,
}

struct ConfigEditorState {
    index: usize,
    name: Entity<InputState>,
    max_plane_size: Entity<SpinSliderState>,
    max_planes_per_row: Entity<SpinSliderState>,
    out_of_gamut_color: Entity<ColorPickerState>,
    plane_controls: Vec<PlaneEditorControls>,
    bar_controls: Vec<BarEditorControls>,
}

pub struct ColorSelectorConfigEditorState {
    configs: Vec<ColorSelectorConfig>,
    selected_config: Option<ConfigEditorState>,
    config_select: Entity<ConfigSelectState>,
}

impl ColorSelectorConfigEditorState {
    pub fn new(
        configs: Vec<ColorSelectorConfig>,
        selected_config: Option<usize>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let config_select = cx.new(|cx| {
            SelectState::new(SearchableVec::new(vec![]), None, window, cx).searchable(false)
        });

        let mut this = Self {
            configs,
            selected_config: None,
            config_select,
        };

        cx.subscribe_in(&this.config_select, window, |this, _, event, window, cx| {
            if let SelectEvent::Confirm(Some(index)) = event {
                this.rebuild_editor(*index, window, cx);
                this.refresh_config_select(window, cx);
                cx.notify();
            }
        })
        .detach();

        if let Some(index) = selected_config {
            this.rebuild_editor(index, window, cx);
        }
        this.refresh_config_select(window, cx);
        this
    }

    pub fn configs(&self) -> &[ColorSelectorConfig] {
        &self.configs
    }

    fn refresh_config_select(&self, window: &mut Window, cx: &mut Context<Self>) {
        let items = SearchableVec::new(
            self.configs
                .iter()
                .enumerate()
                .map(|(index, config)| ConfigItem {
                    index,
                    name: config.name.clone().into(),
                })
                .collect::<Vec<_>>(),
        );
        self.config_select.update(cx, |state, cx| {
            state.set_items(items, window, cx);
            state.set_selected_index(
                self.selected_config
                    .as_ref()
                    .map(|st| IndexPath::new(st.index)),
                window,
                cx,
            );
        });
    }

    fn rebuild_editor(
        &mut self,
        selector_config_index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(config) = self.configs.get(selector_config_index) else {
            self.selected_config = None;
            return;
        };

        let (max_plane_size, max_planes_per_row) = {
            let max_plane_size = cx.new(|cx| {
                SpinSliderState::new(128.0, 512.0, window, cx)
                    .precision(0, window, cx)
                    .value(config.max_plane_size as f32, window, cx)
            });
            let max_planes_per_row = cx.new(|cx| {
                SpinSliderState::new(1.0, 5.0, window, cx)
                    .precision(0, window, cx)
                    .value(config.max_planes_per_row as f32, window, cx)
            });
            (max_plane_size, max_planes_per_row)
        };
        let state = ConfigEditorState {
            index: selector_config_index,
            name: cx.new(|cx| {
                InputState::new(window, cx)
                    .default_value(config.name.to_owned())
                    .placeholder("Config name")
            }),
            max_plane_size,
            max_planes_per_row,
            out_of_gamut_color: {
                let color = config.out_of_gamut_color;
                cx.new(|cx| {
                    ColorPickerState::new(window, cx).default_value(Rgba {
                        r: color.r,
                        g: color.g,
                        b: color.b,
                        a: 1.0,
                    })
                })
            },
            plane_controls: Vec::new(),
            bar_controls: Vec::new(),
        };

        cx.subscribe_in(
            &state.name,
            window,
            move |this, input, event, window, cx| {
                if !matches!(event, InputEvent::Change) {
                    return;
                }

                if let Some(config) = this.configs.get_mut(selector_config_index) {
                    config.name = input.read(cx).value().to_string();
                    this.refresh_config_select(window, cx);
                    cx.notify();
                }
            },
        )
        .detach();

        cx.subscribe_in(
            &state.max_plane_size,
            window,
            move |this, _, event, _, cx| {
                if let SpinSliderEvent::Change(value) = event
                    && let Some(config) = this.configs.get_mut(selector_config_index)
                {
                    config.max_plane_size = value.round() as u32;
                    cx.notify();
                }
            },
        )
        .detach();
        cx.subscribe_in(
            &state.max_planes_per_row,
            window,
            move |this, _, event, _, cx| {
                if let SpinSliderEvent::Change(value) = event
                    && let Some(config) = this.configs.get_mut(selector_config_index)
                {
                    config.max_planes_per_row = value.round() as usize;
                    cx.notify();
                }
            },
        )
        .detach();

        cx.subscribe_in(
            &state.out_of_gamut_color,
            window,
            move |this, _, event, _, cx| {
                let ColorPickerEvent::Change(Some(color)) = event else {
                    return;
                };
                if let Some(config) = this.configs.get_mut(selector_config_index) {
                    let color = Rgba::from(*color);
                    config.out_of_gamut_color = Rgb::new(color.r, color.g, color.b);
                    cx.notify();
                }
            },
        )
        .detach();

        self.selected_config = Some(state);
        self.rebuild_controls(selector_config_index, window, cx);
    }

    fn rebuild_controls(
        &mut self,
        selector_config_index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let config = &self.configs[selector_config_index];
        let n_planes = config.planes.len();
        let n_bars = config.bars.len();

        let mut plane_controls = Vec::new();
        for plane_index in 0..n_planes {
            let controls =
                self.create_plane_controls(selector_config_index, plane_index, window, cx);
            plane_controls.push(controls);
        }
        let mut bar_controls = Vec::new();
        for bar_index in 0..n_bars {
            let controls = self.create_bar_controls(selector_config_index, bar_index, window, cx);
            bar_controls.push(controls);
        }

        let Some(state) = self.selected_config.as_mut() else {
            return;
        };
        state.plane_controls = plane_controls;
        state.bar_controls = bar_controls;
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
        selector_config_index: usize,
        plane_config_index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> PlaneEditorControls {
        let config = &self.configs[selector_config_index].planes[plane_config_index];
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
                && let Some(plane) = this
                    .configs
                    .get_mut(selector_config_index)
                    .and_then(|config| config.planes.get_mut(plane_config_index))
            {
                plane.model = *model;
                cx.notify();
            }
        })
        .detach();
        cx.subscribe_in(&shape, window, move |this, _, event, _, cx| {
            if let SelectEvent::Confirm(Some(shape)) = event
                && let Some(plane) = this
                    .configs
                    .get_mut(selector_config_index)
                    .and_then(|config| config.planes.get_mut(plane_config_index))
            {
                plane.shape = *shape;
                cx.notify();
            }
        })
        .detach();
        cx.subscribe_in(&rotation, window, move |this, _, event, _, cx| {
            if let SpinSliderEvent::Change(rotation) = event
                && let Some(plane) = this
                    .configs
                    .get_mut(selector_config_index)
                    .and_then(|config| config.planes.get_mut(plane_config_index))
            {
                plane.rotation = *rotation;
                cx.notify();
            }
        })
        .detach();
        cx.subscribe_in(&ring_width, window, move |this, _, event, _, cx| {
            if let SpinSliderEvent::Change(width) = event
                && let Some(plane) = this
                    .configs
                    .get_mut(selector_config_index)
                    .and_then(|config| config.planes.get_mut(plane_config_index))
            {
                plane.primary_channel_ring_width = *width;
                cx.notify();
            }
        })
        .detach();
        cx.subscribe_in(&ring_rotation, window, move |this, _, event, _, cx| {
            if let SpinSliderEvent::Change(rotation) = event
                && let Some(plane) = this
                    .configs
                    .get_mut(selector_config_index)
                    .and_then(|config| config.planes.get_mut(plane_config_index))
            {
                plane.ring_rotation = *rotation;
                cx.notify();
            }
        })
        .detach();

        PlaneEditorControls {
            model,
            shape,
            rotation,
            ring_width,
            ring_rotation,
        }
    }

    fn create_bar_controls(
        &mut self,
        selector_config_index: usize,
        bar_config_index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> BarEditorControls {
        let config = &self.configs[selector_config_index].bars[bar_config_index];
        let model = Self::create_model_select(&ColorModel::ALL, config.model, window, cx);
        let bar_height = cx.new(|cx| {
            SpinSliderState::new(10.0, 40.0, window, cx)
                .precision(1, window, cx)
                .value(config.bar_height, window, cx)
        });

        cx.subscribe_in(&model, window, move |this, _, event, _, cx| {
            if let SelectEvent::Confirm(Some(model)) = event
                && let Some(bar) = this
                    .configs
                    .get_mut(selector_config_index)
                    .and_then(|config| config.bars.get_mut(bar_config_index))
            {
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
                && let Some(bar) = this
                    .configs
                    .get_mut(selector_config_index)
                    .and_then(|config| config.bars.get_mut(bar_config_index))
            {
                bar.bar_height = *height;
                cx.notify();
            }
        })
        .detach();

        BarEditorControls { model, bar_height }
    }

    fn add_plane(
        &mut self,
        selector_config_index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let config = GradientPlaneConfig {
            model: ColorModel::Hsv,
            shape: GradientPlaneShape::Square,
            variable_channels: 0b110,
            flip_axis: GradientPlaneFlipAxis::empty(),
            rotation: 0.0,
            show_primary_channel_ring: false,
            primary_channel_ring_width: 20.0,
            ring_bar_saturated_hue_channel: false,
            ring_rotation: 0.0,
            reversed_ring: false,
        };

        let planes = &mut self.configs[selector_config_index].planes;
        planes.push(config);
        let index = planes.len() - 1;
        let controls = self.create_plane_controls(selector_config_index, index, window, cx);
        let Some(state) = self.selected_config.as_mut() else {
            return;
        };
        state.plane_controls.push(controls);
        cx.notify();
    }

    fn remove_plane(
        &mut self,
        selector_config_index: usize,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.configs[selector_config_index].planes.remove(index);
        self.rebuild_controls(selector_config_index, window, cx);
        cx.notify();
    }

    fn move_plane(
        &mut self,
        selector_config_index: usize,
        index: usize,
        offset: isize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let target = index.saturating_add_signed(offset);
        let planes = &mut self.configs[selector_config_index].planes;
        if target >= planes.len() {
            return;
        }
        planes.swap(index, target);
        self.rebuild_controls(selector_config_index, window, cx);
        cx.notify();
    }

    fn add_bar(
        &mut self,
        selector_config_index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let config = GradientBarConfig {
            model: ColorModel::Rgb,
            channel: 0,
            bar_height: 20.0,
            show_channel_label: true,
            show_precise_spin_box: true,
            show_primary_channel_lock: false,
        };
        let bars = &mut self.configs[selector_config_index].bars;
        bars.push(config);
        let index = bars.len() - 1;
        let controls = self.create_bar_controls(selector_config_index, index, window, cx);
        let Some(state) = self.selected_config.as_mut() else {
            return;
        };
        state.bar_controls.push(controls);
        cx.notify();
    }

    fn remove_bar(
        &mut self,
        selector_config_index: usize,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.configs[selector_config_index].bars.remove(index);
        self.rebuild_controls(selector_config_index, window, cx);
        cx.notify();
    }

    fn move_bar(
        &mut self,
        selector_config_index: usize,
        index: usize,
        offset: isize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let target = index.saturating_add_signed(offset);
        let bars = &mut self.configs[selector_config_index].bars;
        if target >= bars.len() {
            return;
        }
        bars.swap(index, target);
        self.rebuild_controls(selector_config_index, window, cx);
        cx.notify();
    }

    fn add_config(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.configs.push(ColorSelectorConfig {
            name: format!("Config {}", self.configs.len() + 1),
            max_plane_size: 512,
            max_planes_per_row: 2,
            planes: Vec::new(),
            bars: Vec::new(),
            out_of_gamut_color: Rgb::new(0.5, 0.5, 0.5),
            use_out_of_gamut_color: true,
            clip_to_gamut: false,
        });
        let index = self.configs.len() - 1;
        self.rebuild_editor(index, window, cx);
        self.refresh_config_select(window, cx);
        cx.notify();
    }

    fn remove_config(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(index) = self.selected_config.as_ref().map(|state| state.index) else {
            return;
        };

        self.configs.remove(index);
        if self.configs.is_empty() {
            self.selected_config = None;
        } else {
            self.rebuild_editor(index.min(self.configs.len() - 1), window, cx);
        }
        self.refresh_config_select(window, cx);
        cx.notify();
    }

    fn move_config(&mut self, offset: isize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(index) = self.selected_config.as_ref().map(|state| state.index) else {
            return;
        };
        let target = index.saturating_add_signed(offset);
        if target >= self.configs.len() || target == index {
            return;
        }

        self.configs.swap(index, target);
        self.rebuild_editor(target, window, cx);
        self.refresh_config_select(window, cx);
        cx.notify();
    }

    fn render_plane(
        &self,
        selector_config_index: usize,
        index: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(state) = &self.selected_config else {
            return div().into_any_element();
        };

        let config = self.configs[selector_config_index].planes[index].clone();
        let controls = state.plane_controls[index].clone();
        let labels = config.model.channel_labels();
        let primary_channel = (0..3)
            .find(|channel| config.variable_channels & (1u8 << channel) == 0)
            .unwrap_or(0);

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
                                Button::new(format!("plane-up-{index}"))
                                    .xsmall()
                                    .label("Up")
                                    .disabled(index == 0)
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        this.move_plane(
                                            selector_config_index,
                                            index,
                                            -1,
                                            window,
                                            cx,
                                        )
                                    })),
                            )
                            .child(
                                Button::new(format!("plane-down-{index}"))
                                    .xsmall()
                                    .label("Down")
                                    .disabled(
                                        index + 1
                                            == self.configs[selector_config_index].planes.len(),
                                    )
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        this.move_plane(selector_config_index, index, 1, window, cx)
                                    })),
                            )
                            .child(
                                Button::new(format!("plane-remove-{index}"))
                                    .xsmall()
                                    .danger()
                                    .label("Remove")
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        this.remove_plane(selector_config_index, index, window, cx)
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
                        RadioGroup::horizontal(format!("plane-primary-{index}"))
                            .children(labels.iter().copied())
                            .selected_index(Some(primary_channel))
                            .on_click(cx.listener(move |this, channel: &usize, _, cx| {
                                this.configs[selector_config_index].planes[index]
                                    .variable_channels = 0b111u8 & !(1u8 << *channel);
                                cx.notify();
                            })),
                    ),
            )
            .child(
                h_flex()
                    .gap_4()
                    .child(
                        Checkbox::new(format!("plane-flip-x-{index}"))
                            .label("Flip X")
                            .checked(config.flip_axis.contains(GradientPlaneFlipAxis::X))
                            .on_click(cx.listener(move |this, checked, _, cx| {
                                this.configs[selector_config_index].planes[index]
                                    .flip_axis
                                    .set(GradientPlaneFlipAxis::X, *checked);
                                cx.notify();
                            })),
                    )
                    .child(
                        Checkbox::new(format!("plane-flip-y-{index}"))
                            .label("Flip Y")
                            .checked(config.flip_axis.contains(GradientPlaneFlipAxis::Y))
                            .on_click(cx.listener(move |this, checked, _, cx| {
                                this.configs[selector_config_index].planes[index]
                                    .flip_axis
                                    .set(GradientPlaneFlipAxis::Y, *checked);
                                cx.notify();
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
                        Checkbox::new(format!("plane-show-ring-{index}"))
                            .label("Primary channel ring")
                            .checked(config.show_primary_channel_ring)
                            .on_click(cx.listener(move |this, checked, _, cx| {
                                this.configs[selector_config_index].planes[index]
                                    .show_primary_channel_ring = *checked;
                                cx.notify();
                            })),
                    )
                    .child(
                        Checkbox::new(format!("plane-saturated-primary-channel-{index}"))
                            .label("Saturated primary channel")
                            .checked(config.ring_bar_saturated_hue_channel)
                            .on_click(cx.listener(move |this, checked, _, cx| {
                                this.configs[selector_config_index].planes[index]
                                    .ring_bar_saturated_hue_channel = *checked;
                                cx.notify();
                            })),
                    )
                    .child(
                        Checkbox::new(format!("plane-reversed-ring-{index}"))
                            .label("Reversed ring")
                            .checked(config.reversed_ring)
                            .disabled(!config.show_primary_channel_ring)
                            .on_click(cx.listener(move |this, checked, _, cx| {
                                this.configs[selector_config_index].planes[index].reversed_ring =
                                    *checked;
                                cx.notify();
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

    fn render_bar(
        &self,
        selector_config_index: usize,
        index: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let config = self.configs[selector_config_index].bars[index].clone();
        let labels = config.model.channel_labels();
        let Some(state) = self.selected_config.as_ref() else {
            return div().into_any_element();
        };
        let controls = state.bar_controls[index].clone();

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
                                Button::new(format!("bar-up-{index}"))
                                    .xsmall()
                                    .label("Up")
                                    .disabled(index == 0)
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        this.move_bar(selector_config_index, index, -1, window, cx)
                                    })),
                            )
                            .child(
                                Button::new(format!("bar-down-{index}"))
                                    .xsmall()
                                    .label("Down")
                                    .disabled(
                                        index + 1 == self.configs[selector_config_index].bars.len(),
                                    )
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        this.move_bar(selector_config_index, index, 1, window, cx)
                                    })),
                            )
                            .child(
                                Button::new(format!("bar-remove-{index}"))
                                    .xsmall()
                                    .danger()
                                    .label("Remove")
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        this.remove_bar(selector_config_index, index, window, cx)
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
                        RadioGroup::horizontal(format!("bar-channel-{index}"))
                            .children(labels.iter().copied())
                            .selected_index(Some(config.channel as usize))
                            .on_click(cx.listener(move |this, channel: &usize, _, cx| {
                                this.configs[selector_config_index].bars[index].channel =
                                    *channel as u8;
                                cx.notify();
                            })),
                    ),
            )
            .child(
                h_flex()
                    .gap_4()
                    .child(
                        Checkbox::new(format!("bar-label-{index}"))
                            .label("Channel label")
                            .checked(config.show_channel_label)
                            .on_click(cx.listener(move |this, checked, _, cx| {
                                this.configs[selector_config_index].bars[index]
                                    .show_channel_label = *checked;
                                cx.notify();
                            })),
                    )
                    .child(
                        Checkbox::new(format!("bar-spin-{index}"))
                            .label("Precise spin box")
                            .checked(config.show_precise_spin_box)
                            .on_click(cx.listener(move |this, checked, _, cx| {
                                this.configs[selector_config_index].bars[index]
                                    .show_precise_spin_box = *checked;
                                cx.notify();
                            })),
                    )
                    .child(
                        Checkbox::new(format!("bar-lock-{index}"))
                            .label("Primary channel lock")
                            .checked(config.show_primary_channel_lock)
                            .on_click(cx.listener(move |this, checked, _, cx| {
                                this.configs[selector_config_index].bars[index]
                                    .show_primary_channel_lock = *checked;
                                cx.notify();
                            })),
                    ),
            )
            .into_any_element()
    }
}

impl Render for ColorSelectorConfigEditorState {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let active_content = if let Some(state) = self.selected_config.as_ref() {
            let selector_config_index = state.index;
            let config = &self.configs[state.index];
            let planes = (0..config.planes.len())
                .map(|plane_index| self.render_plane(state.index, plane_index, cx))
                .collect::<Vec<_>>();
            let bars = (0..config.bars.len())
                .map(|bar_index| self.render_bar(state.index, bar_index, cx))
                .collect::<Vec<_>>();

            v_flex()
                .gap_3()
                .child(
                    h_flex()
                        .gap_2()
                        .child(div().w(px(110.)).child("Name"))
                        .child(div().flex_1().child(Input::new(&state.name).small())),
                )
                .child(
                    SpinSlider::new(&state.max_plane_size)
                        .small()
                        .prefix("Max plane size: ")
                        .suffix(" px"),
                )
                .child(
                    SpinSlider::new(&state.max_planes_per_row)
                        .small()
                        .prefix("Max planes per row: "),
                )
                .child(
                    h_flex()
                        .gap_4()
                        .child(
                            Checkbox::new("color-selector-out-of-gamut-color")
                                .label("Out-of-gamut color")
                                .checked(config.use_out_of_gamut_color)
                                .on_click(cx.listener(move |this, checked: &bool, _, cx| {
                                    this.configs[selector_config_index].use_out_of_gamut_color =
                                        *checked;
                                    cx.notify();
                                })),
                        )
                        .child(ColorPicker::new(&state.out_of_gamut_color).small())
                        .child(
                            Checkbox::new("color-selector-clip-to-gamut")
                                .label("Clip to gamut")
                                .checked(config.clip_to_gamut)
                                .on_click(cx.listener(move |this, checked, _, cx| {
                                    this.configs[selector_config_index].clip_to_gamut = *checked;
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
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        this.add_plane(selector_config_index, window, cx)
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
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        this.add_bar(selector_config_index, window, cx)
                                    })),
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
                                .disabled(
                                    self.selected_config
                                        .as_ref()
                                        .is_none_or(|index| index.index == 0),
                                )
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.move_config(-1, window, cx)
                                })),
                        )
                        .child(
                            Button::new("move-color-selector-config-down")
                                .small()
                                .label("Down")
                                .disabled(
                                    self.selected_config
                                        .as_ref()
                                        .is_none_or(|st| st.index + 1 == self.configs.len()),
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
                                .disabled(self.selected_config.is_none())
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
