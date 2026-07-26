use gpui::{
    AnyElement, App, AppContext, ClickEvent, Context, DragMoveEvent, Empty, Entity, EntityId,
    EventEmitter, FocusHandle, Focusable, InteractiveElement, IntoElement, KeyBinding, MouseButton,
    MouseDownEvent, MouseUpEvent, ParentElement, Render, RenderOnce, StatefulInteractiveElement,
    StyleRefinement, Styled, Subscription, TextAlign, Window, actions, div, prelude::FluentBuilder,
    px, relative,
};
use gpui_component::{
    ActiveTheme, Disableable, IconName, Sizable, Size, StyleSized, StyledExt,
    button::Button,
    input::{Escape, Input, InputEvent, InputState, StepAction},
    slider::SliderScale,
};

const KEY_CONTEXT: &str = "SpinSlider";

actions!(spin_slider, [Increment, Decrement]);

pub fn init(cx: &mut App) {
    cx.bind_keys(vec![
        KeyBinding::new("up", Increment, Some(KEY_CONTEXT)),
        KeyBinding::new("down", Decrement, Some(KEY_CONTEXT)),
        KeyBinding::new("escape", Escape, Some(KEY_CONTEXT)),
    ]);
}

#[derive(Clone)]
struct DragSlider(EntityId);

impl Render for DragSlider {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        Empty
    }
}

pub enum SpinSliderEvent {
    Change(f32),
    Release(f32),
}

pub struct SpinSliderState {
    input_state: Entity<InputState>,

    value: f32,
    min: f32,
    max: f32,
    step: f32,
    precision: usize,
    scale: SliderScale,
    // TODO Hold shift to slide more precisely
    pending_edit: bool,
    editing: bool,
    value_before_edit: f32,

    _subscriptions: Vec<Subscription>,
}

impl SpinSliderState {
    const DEFAULT_PRECISION: usize = 2;

    pub fn new(min: f32, max: f32, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let input_state = cx.new(|cx| {
            InputState::new(window, cx).default_value(format!(
                "{:.*}",
                Self::DEFAULT_PRECISION,
                min
            ))
        });

        let _subscriptions = vec![cx.subscribe_in(&input_state, window, Self::on_input_event)];

        Self {
            input_state: input_state.clone(),
            value: min,
            min,
            max,
            step: 0.01,
            scale: SliderScale::Linear,
            precision: Self::DEFAULT_PRECISION,
            editing: false,
            value_before_edit: min,
            pending_edit: false,
            _subscriptions,
        }
    }

    pub fn new_01(window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self::new(0.0, 1.0, window, cx)
    }

    pub fn new_percent(window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self::new(0.0, 100.0, window, cx).precision(0, window, cx)
    }

    pub fn step(mut self, step: f32, cx: &mut Context<Self>) -> Self {
        self.step = step;
        self.set_value(self.value, cx);
        self
    }

    pub fn precision(
        mut self,
        precision: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        self.precision = precision;
        self.step = 0.1f32.powf(self.precision as f32);
        self.sync_input_from_value(window, cx);
        self
    }

    pub fn scale(mut self, scale: SliderScale) -> Self {
        self.scale = scale;
        self
    }

    pub fn value(mut self, value: f32, window: &mut Window, cx: &mut Context<Self>) -> Self {
        self.value = value.clamp(self.min, self.max);
        self.sync_input_from_value(window, cx);
        self
    }

    pub fn editing(&self) -> bool {
        self.editing
    }

    pub fn set_value(&mut self, value: f32, cx: &mut Context<Self>) {
        self.value = value.clamp(self.min, self.max);
        cx.emit(SpinSliderEvent::Change(self.value));
    }

    fn step_value(&mut self, action: StepAction, window: &mut Window, cx: &mut Context<Self>) {
        let delta = match action {
            StepAction::Increment => self.step,
            StepAction::Decrement => -self.step,
        };
        self.set_value(self.value + delta, cx);
        self.sync_input_from_value(window, cx);
    }

    fn commit_and_exit_edit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Ok(parsed) = self.input_state.read(cx).unmask_value().parse::<f32>() else {
            return;
        };

        let value = parsed.clamp(self.min, self.max);
        self.editing = false;
        self.value = value;
        self.sync_input_from_value(window, cx);

        cx.emit(SpinSliderEvent::Change(value));
        cx.emit(SpinSliderEvent::Release(value));
        cx.notify();
    }

    fn on_input_event(
        &mut self,
        _: &Entity<InputState>,
        event: &InputEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            InputEvent::PressEnter { .. } | InputEvent::Blur => {
                if self.editing {
                    self.commit_and_exit_edit(window, cx);
                }
            }
            InputEvent::Focus | InputEvent::Change => {}
        }
    }

    fn on_minus_click(&mut self, _: &ClickEvent, window: &mut Window, cx: &mut Context<Self>) {
        self.step_value(StepAction::Decrement, window, cx);
    }

    fn on_plus_click(&mut self, _: &ClickEvent, window: &mut Window, cx: &mut Context<Self>) {
        self.step_value(StepAction::Increment, window, cx);
    }

    fn on_action_increment(&mut self, _: &Increment, window: &mut Window, cx: &mut Context<Self>) {
        self.step_value(StepAction::Increment, window, cx);
    }

    fn on_action_decrement(&mut self, _: &Decrement, window: &mut Window, cx: &mut Context<Self>) {
        self.step_value(StepAction::Decrement, window, cx);
    }

    fn on_action_escape(&mut self, _: &Escape, window: &mut Window, cx: &mut Context<Self>) {
        if !self.editing {
            return;
        }
        self.editing = false;
        self.value = self.value_before_edit;
        self.sync_input_from_value(window, cx);
        cx.notify();
        cx.stop_propagation();
    }

    fn on_drag_move(
        &mut self,
        event: &DragMoveEvent<DragSlider>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if event.drag(cx).0 != cx.entity_id() {
            return;
        }

        self.pending_edit = false;
        cx.stop_propagation();
        let percentage =
            (window.mouse_position().x - event.bounds.origin.x) / event.bounds.size.width;
        let value = self.percentage_to_value(percentage);
        let step = 0.1f32.powf(self.precision as f32);
        let snapped = (value / step).round() * step;
        self.set_value(snapped.clamp(self.min, self.max), cx);
        self.sync_input_from_value(window, cx);
    }

    fn on_mouse_down(&mut self, _: &MouseDownEvent, _: &mut Window, _: &mut Context<Self>) {
        self.pending_edit = true;
    }

    fn on_mouse_up(&mut self, _: &MouseUpEvent, window: &mut Window, cx: &mut Context<Self>) {
        if self.pending_edit {
            if self.editing {
                return;
            }

            self.value_before_edit = self.value;
            self.editing = true;
            self.sync_input_from_value(window, cx);
            self.input_state.update(cx, |state, cx| {
                state.focus(window, cx);
            });
            cx.notify();
        } else {
            cx.stop_propagation();
            cx.emit(SpinSliderEvent::Release(self.value));
        }
    }

    fn sync_input_from_value(&self, window: &mut Window, cx: &mut Context<Self>) {
        let text = format!("{:.*}", self.precision, self.value);
        self.input_state.update(cx, |state, cx| {
            state.set_value(text, window, cx);
        });
    }

    // Copied from gpui-component
    fn percentage_to_value(&self, percentage: f32) -> f32 {
        match self.scale {
            SliderScale::Linear => self.min + (self.max - self.min) * percentage,
            SliderScale::Logarithmic => {
                // when percentage is 0, this simplifies to (max/min)^0 * min = 1 * min = min
                // when percentage is 1, this simplifies to (max/min)^1 * min = (max*min)/min = max
                // we clamp just to make sure we don't have issue with floating point precision
                let base = self.max / self.min;
                (base.powf(percentage) * self.min).clamp(self.min, self.max)
            }
        }
    }

    // Copied from gpui-component
    fn value_to_percentage(&self, value: f32) -> f32 {
        match self.scale {
            SliderScale::Linear => {
                let range = self.max - self.min;
                if range <= 0.0 {
                    0.0
                } else {
                    (value - self.min) / range
                }
            }
            SliderScale::Logarithmic => {
                let base = self.max / self.min;
                (value / self.min).log(base).clamp(0.0, 1.0)
            }
        }
    }
}

impl EventEmitter<SpinSliderEvent> for SpinSliderState {}

impl Focusable for SpinSliderState {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.input_state.focus_handle(cx)
    }
}

#[derive(IntoElement)]
pub struct SpinSlider {
    state: Entity<SpinSliderState>,
    size: Size,
    disabled: bool,
    style: StyleRefinement,
    prefix: Option<AnyElement>,
    suffix: Option<AnyElement>,
}

impl SpinSlider {
    pub fn new(state: &Entity<SpinSliderState>) -> Self {
        Self {
            state: state.clone(),
            size: Size::default(),
            disabled: false,
            style: StyleRefinement::default(),
            prefix: None,
            suffix: None,
        }
    }

    pub fn prefix(mut self, prefix: impl IntoElement) -> Self {
        self.prefix = Some(prefix.into_any_element());
        self
    }

    pub fn suffix(mut self, suffix: impl IntoElement) -> Self {
        self.suffix = Some(suffix.into_any_element());
        self
    }
}

impl Disableable for SpinSlider {
    fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}

impl Styled for SpinSlider {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl Sizable for SpinSlider {
    fn with_size(mut self, size: impl Into<Size>) -> Self {
        self.size = size.into();
        self
    }
}

impl Focusable for SpinSlider {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.state.read(cx).input_state.focus_handle(cx)
    }
}

impl RenderOnce for SpinSlider {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme();
        let state_ref = self.state.read(cx);
        let input_state = state_ref.input_state.clone();
        let fill_fraction = state_ref.value_to_percentage(state_ref.value);
        let editing = state_ref.editing;
        let value = state_ref.value;
        let precision = state_ref.precision;
        let size = self.size;
        let disabled = self.disabled;

        let bar_color = self
            .style
            .background
            .clone()
            .and_then(|bg| bg.color())
            .unwrap_or(theme.accent.into());
        let track_color = theme.input.opacity(0.12);
        let text_color = theme.foreground;

        let slider_input = div()
            .id(("spin-slider-middle", self.state.entity_id()))
            .relative()
            .flex_1()
            .h_full()
            .min_w(px(0.))
            .overflow_hidden()
            .bg(track_color)
            .border_color(theme.input)
            .border_1()
            .child(
                div()
                    .absolute()
                    .top(px(0.))
                    .left(px(0.))
                    .h_full()
                    .w(relative(fill_fraction))
                    .bg(bar_color),
            )
            .when(editing, |this| {
                this.child(
                    Input::new(&input_state)
                        .appearance(false)
                        .bordered(false)
                        .with_size(size)
                        .disabled(disabled)
                        .gap_0()
                        .rounded_none()
                        .text_align(TextAlign::Center),
                )
            })
            .when(!editing, |this| {
                this.when(!disabled, |this| {
                    this.cursor_text()
                        .on_drag(DragSlider(self.state.entity_id()), |drag, _, _, cx| {
                            cx.stop_propagation();
                            cx.new(|_| drag.clone())
                        })
                        .on_drag_move(
                            window.listener_for(&self.state, SpinSliderState::on_drag_move),
                        )
                        .on_mouse_down(
                            MouseButton::Left,
                            window.listener_for(&self.state, SpinSliderState::on_mouse_down),
                        )
                        .on_mouse_up(
                            MouseButton::Left,
                            window.listener_for(&self.state, SpinSliderState::on_mouse_up),
                        )
                })
                .child(
                    div()
                        .input_h(size)
                        .relative()
                        .flex_1()
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_color(text_color)
                        .when_some(self.prefix, |this, pre| this.child(pre))
                        .child(format!("{:.*}", precision, value))
                        .when_some(self.suffix, |this, suf| this.child(suf)),
                )
            });

        div()
            .id(("spin-slider", self.state.entity_id()))
            .key_context(KEY_CONTEXT)
            .on_action(window.listener_for(&self.state, SpinSliderState::on_action_increment))
            .on_action(window.listener_for(&self.state, SpinSliderState::on_action_decrement))
            .on_action(window.listener_for(&self.state, SpinSliderState::on_action_escape))
            .h_flex()
            .flex_1()
            .rounded(cx.theme().radius)
            .refine_style(&self.style)
            .when(self.disabled, |this| this.opacity(0.5))
            .child(
                Button::new("minus")
                    .outline()
                    .with_size(size)
                    .icon(IconName::Minus)
                    .compact()
                    .tab_stop(false)
                    .disabled(disabled)
                    .border_0()
                    // TODO Button styles are incorrect. They have rounded corners and borders at every direction
                    //      Fix this if gpui-component make border_edges and border_corners public
                    .on_click(window.listener_for(&self.state, SpinSliderState::on_minus_click)),
            )
            .child(slider_input)
            .child(
                Button::new("plus")
                    .outline()
                    .with_size(size)
                    .icon(IconName::Plus)
                    .compact()
                    .tab_stop(false)
                    .disabled(disabled)
                    .on_click(window.listener_for(&self.state, SpinSliderState::on_plus_click)),
            )
    }
}
