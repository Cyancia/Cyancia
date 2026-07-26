use std::f32::consts::TAU;

use cyancia_widgets::spin_slider::{SpinSlider, SpinSliderEvent, SpinSliderState};
use gpui::{
    AnyElement, App, AppContext, Context, Entity, EventEmitter, InteractiveElement, IntoElement,
    ParentElement, Render, RenderOnce, SharedString, Styled, Window, div, px,
};
use gpui_component::{
    ActiveTheme, Disableable, IndexPath, Sizable,
    button::{Button, ButtonVariants},
    checkbox::Checkbox,
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
    pub planes: Vec<GradientPlaneConfig>,
    pub bars: Vec<GradientBarConfig>,
}

#[derive(Debug, Clone)]
pub struct GradientPlaneConfig {
    pub model: ColorModel,
    pub shape: GradientPlaneShape,
    pub variable_channels: u8,
    pub flip_axis: GradientPlaneFlipAxis,
    pub rotation: f32,
    pub show_primary_channel_ring: bool,
    pub saturated_primary_channel_ring: bool,
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
    pub show_channel_label: bool,
    pub show_precise_spin_box: bool,
    pub show_primary_channel_lock: bool,
}

#[derive(Clone)]
struct ColorModelItem(ColorModel);

impl SearchableListItem for ColorModelItem {
    type Value = ColorModel;

    fn title(&self) -> SharedString {
        self.0.to_string().into()
    }

    fn value(&self) -> &Self::Value {
        &self.0
    }
}

#[derive(Clone)]
struct PlaneShapeItem(GradientPlaneShape);

impl SearchableListItem for PlaneShapeItem {
    type Value = GradientPlaneShape;

    fn title(&self) -> SharedString {
        self.0.to_string().into()
    }

    fn value(&self) -> &Self::Value {
        &self.0
    }
}

type ColorModelSelectState = SelectState<SearchableVec<ColorModelItem>>;
type PlaneShapeSelectState = SelectState<SearchableVec<PlaneShapeItem>>;

#[derive(Clone)]
struct PlaneEditorControls {
    id: u64,
    model: Entity<ColorModelSelectState>,
    shape: Entity<PlaneShapeSelectState>,
    rotation: Entity<SpinSliderState>,
    ring_rotation: Entity<SpinSliderState>,
}

#[derive(Clone)]
struct BarEditorControls {
    id: u64,
    model: Entity<ColorModelSelectState>,
}

pub struct ColorSelectorConfigEditorState {
    config: ColorSelectorConfig,
    name: Entity<InputState>,
    plane_controls: Vec<PlaneEditorControls>,
    bar_controls: Vec<BarEditorControls>,
    next_control_id: u64,
}

impl ColorSelectorConfigEditorState {
    pub fn new(config: ColorSelectorConfig, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let name = cx.new(|cx| {
            InputState::new(window, cx)
                .default_value(config.name.clone())
                .placeholder("Config name")
        });

        cx.subscribe_in(&name, window, |this, input, event, _, cx| {
            if matches!(event, InputEvent::Change) {
                this.config.name = input.read(cx).value().to_string();
                cx.notify();
            }
        })
        .detach();

        let plane_configs = config.planes.clone();
        let bar_configs = config.bars.clone();
        let mut this = Self {
            config,
            name,
            plane_controls: Vec::with_capacity(plane_configs.len()),
            bar_controls: Vec::with_capacity(bar_configs.len()),
            next_control_id: 0,
        };

        for plane in &plane_configs {
            let controls = this.create_plane_controls(plane, window, cx);
            this.plane_controls.push(controls);
        }
        for bar in &bar_configs {
            let controls = this.create_bar_controls(bar, window, cx);
            this.bar_controls.push(controls);
        }

        this
    }

    pub fn config(&self) -> &ColorSelectorConfig {
        &self.config
    }

    fn next_control_id(&mut self) -> u64 {
        let id = self.next_control_id;
        self.next_control_id += 1;
        id
    }

    fn create_model_select(
        models: &[ColorModel],
        selected: ColorModel,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<ColorModelSelectState> {
        let items = models
            .iter()
            .copied()
            .map(ColorModelItem)
            .collect::<Vec<_>>();
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
        let id = self.next_control_id();
        let model = Self::create_model_select(&ColorModel::PLANE_MODELS, config.model, window, cx);
        let shapes = [GradientPlaneShape::Square, GradientPlaneShape::Triangle];
        let shape_items = shapes
            .iter()
            .copied()
            .map(PlaneShapeItem)
            .collect::<Vec<_>>();
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
        let ring_rotation = cx.new(|cx| {
            SpinSliderState::new(0.0, TAU, window, cx)
                .precision(3, window, cx)
                .value(config.ring_rotation.rem_euclid(TAU), window, cx)
        });

        cx.subscribe_in(&model, window, move |this, _, event, _, cx| {
            if let SelectEvent::Confirm(Some(model)) = event {
                if let Some(index) = this.plane_index(id) {
                    this.config.planes[index].model = *model;
                    cx.notify();
                }
            }
        })
        .detach();
        cx.subscribe_in(&shape, window, move |this, _, event, _, cx| {
            if let SelectEvent::Confirm(Some(shape)) = event {
                if let Some(index) = this.plane_index(id) {
                    this.config.planes[index].shape = *shape;
                    cx.notify();
                }
            }
        })
        .detach();
        cx.subscribe_in(&rotation, window, move |this, _, event, _, cx| {
            if let SpinSliderEvent::Change(rotation) = event {
                if let Some(index) = this.plane_index(id) {
                    this.config.planes[index].rotation = *rotation;
                    cx.notify();
                }
            }
        })
        .detach();
        cx.subscribe_in(&ring_rotation, window, move |this, _, event, _, cx| {
            if let SpinSliderEvent::Change(rotation) = event {
                if let Some(index) = this.plane_index(id) {
                    this.config.planes[index].ring_rotation = *rotation;
                    cx.notify();
                }
            }
        })
        .detach();

        PlaneEditorControls {
            id,
            model,
            shape,
            rotation,
            ring_rotation,
        }
    }

    fn create_bar_controls(
        &mut self,
        config: &GradientBarConfig,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> BarEditorControls {
        let id = self.next_control_id();
        let model = Self::create_model_select(&ColorModel::ALL, config.model, window, cx);

        cx.subscribe_in(&model, window, move |this, _, event, _, cx| {
            if let SelectEvent::Confirm(Some(model)) = event {
                if let Some(index) = this.bar_index(id) {
                    let bar = &mut this.config.bars[index];
                    bar.model = *model;
                    bar.channel = bar
                        .channel
                        .min(model.channel_labels().len().saturating_sub(1) as u8);
                    cx.notify();
                }
            }
        })
        .detach();

        BarEditorControls { id, model }
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
            saturated_primary_channel_ring: false,
            ring_rotation: 0.0,
            reversed_ring: false,
        };
        let controls = self.create_plane_controls(&config, window, cx);
        self.config.planes.push(config);
        self.plane_controls.push(controls);
        cx.notify();
    }

    fn remove_plane(&mut self, index: usize, cx: &mut Context<Self>) {
        self.config.planes.remove(index);
        self.plane_controls.remove(index);
        cx.notify();
    }

    fn move_plane(&mut self, index: usize, offset: isize, cx: &mut Context<Self>) {
        let target = index.saturating_add_signed(offset);
        if target >= self.config.planes.len() {
            return;
        }
        self.config.planes.swap(index, target);
        self.plane_controls.swap(index, target);
        cx.notify();
    }

    fn add_bar(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let config = GradientBarConfig {
            model: ColorModel::Rgb,
            channel: 0,
            show_channel_label: true,
            show_precise_spin_box: true,
            show_primary_channel_lock: false,
        };
        let controls = self.create_bar_controls(&config, window, cx);
        self.config.bars.push(config);
        self.bar_controls.push(controls);
        cx.notify();
    }

    fn remove_bar(&mut self, index: usize, cx: &mut Context<Self>) {
        self.config.bars.remove(index);
        self.bar_controls.remove(index);
        cx.notify();
    }

    fn move_bar(&mut self, index: usize, offset: isize, cx: &mut Context<Self>) {
        let target = index.saturating_add_signed(offset);
        if target >= self.config.bars.len() {
            return;
        }
        self.config.bars.swap(index, target);
        self.bar_controls.swap(index, target);
        cx.notify();
    }

    fn primary_channel(variable_channels: u8) -> usize {
        (0..3)
            .find(|channel| variable_channels & (1u8 << channel) == 0)
            .unwrap_or(0)
    }

    fn render_plane(&self, index: usize, cx: &mut Context<Self>) -> AnyElement {
        let config = self.config.planes[index].clone();
        let controls = self.plane_controls[index].clone();
        let labels = config.model.channel_labels();
        let primary_channel = Self::primary_channel(config.variable_channels);
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
                                    .disabled(index + 1 == self.config.planes.len())
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
                                    this.config.planes[index].variable_channels =
                                        0b111u8 & !(1u8 << *channel);
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
                                    this.config.planes[index]
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
                                    this.config.planes[index]
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
                                    this.config.planes[index].show_primary_channel_ring = *checked;
                                    cx.notify();
                                }
                            })),
                    )
                    .child(
                        Checkbox::new(format!("plane-saturated-ring-{id}"))
                            .label("Saturated ring")
                            .checked(config.saturated_primary_channel_ring)
                            .disabled(!config.show_primary_channel_ring)
                            .on_click(cx.listener(move |this, checked, _, cx| {
                                if let Some(index) = this.plane_index(id) {
                                    this.config.planes[index].saturated_primary_channel_ring =
                                        *checked;
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
                                    this.config.planes[index].reversed_ring = *checked;
                                    cx.notify();
                                }
                            })),
                    ),
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
        let config = self.config.bars[index].clone();
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
                                    .disabled(index + 1 == self.config.bars.len())
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
                h_flex()
                    .gap_2()
                    .child(div().w(px(110.)).child("Channel"))
                    .child(
                        RadioGroup::horizontal(format!("bar-channel-{id}"))
                            .children(labels.iter().copied())
                            .selected_index(Some(config.channel as usize))
                            .on_click(cx.listener(move |this, channel: &usize, _, cx| {
                                if let Some(index) = this.bar_index(id) {
                                    this.config.bars[index].channel = *channel as u8;
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
                                    this.config.bars[index].show_channel_label = *checked;
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
                                    this.config.bars[index].show_precise_spin_box = *checked;
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
                                    this.config.bars[index].show_primary_channel_lock = *checked;
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
        let planes = (0..self.config.planes.len())
            .map(|index| self.render_plane(index, cx))
            .collect::<Vec<_>>();
        let bars = (0..self.config.bars.len())
            .map(|index| self.render_bar(index, cx))
            .collect::<Vec<_>>();

        let content = v_flex()
            .id("color-selector-config-editor-content")
            .flex_1()
            .gap_3()
            .p_3()
            .overflow_y_scrollbar()
            .child(
                h_flex()
                    .gap_2()
                    .child(div().w(px(110.)).child("Name"))
                    .child(div().flex_1().child(Input::new(&self.name).small())),
            )
            .child(
                v_flex()
                    .gap_2()
                    .child(
                        h_flex().justify_between().child("Planes").child(
                            Button::new("add-color-selector-plane")
                                .small()
                                .label("Add plane")
                                .on_click(
                                    cx.listener(|this, _, window, cx| this.add_plane(window, cx)),
                                ),
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
            );

        v_flex().size_full().child(content).child(
            h_flex().flex_shrink_0().justify_end().p_3().child(
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
}

impl EventEmitter<ColorSelectorConfigEvent> for ColorSelectorConfigEditorState {}

#[derive(IntoElement)]
pub struct ColorSelectorConfigEditor {
    state: Entity<ColorSelectorConfigEditorState>,
}

impl ColorSelectorConfigEditor {
    pub fn new(state: &Entity<ColorSelectorConfigEditorState>) -> Self {
        Self {
            state: state.clone(),
        }
    }
}

impl RenderOnce for ColorSelectorConfigEditor {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        div().size_full().child(self.state)
    }
}
