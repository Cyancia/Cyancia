use std::{
    cell::{Cell, RefCell},
    panic::Location,
    rc::Rc,
};

use cyancia_math::curve::CubicCurve;
use glam::Vec2;
use gpui::{
    App, BorderStyle, Bounds, ContentMask, Context, Corners, Element, ElementId, Entity,
    EventEmitter, FocusHandle, GlobalElementId, HitboxBehavior, Hsla, InspectorElementId,
    InteractiveElement, IntoElement, KeyBinding, KeyDownEvent, LayoutId, MouseDownEvent,
    MouseMoveEvent, MouseUpEvent, PaintQuad, ParentElement, PathBuilder, Pixels, Point, RenderOnce,
    Style, Styled, Window, actions, div, fill, hsla, outline, point, px, relative, size,
};

const DEFAULT_ID: &str = "curve-edit-widget";
const KEY_CONTEXT: &str = "CurveEditWidget";
const MIN_POINT_GAP: f32 = 0.001;

actions!(curve_edit, [DeleteSelectedControlPoint]);

pub(crate) fn init(cx: &mut App) {
    cx.bind_keys(vec![KeyBinding::new(
        "delete",
        DeleteSelectedControlPoint,
        Some(KEY_CONTEXT),
    )]);
}

#[derive(Clone)]
pub struct CurveEditState {
    selected_index: Option<usize>,
    drag_index: Option<usize>,
    curve: CubicCurve,
    focus_handle: FocusHandle,
}

impl CurveEditState {
    pub fn new(curve: CubicCurve, cx: &mut Context<Self>) -> Self {
        Self {
            selected_index: None,
            drag_index: None,
            curve,
            focus_handle: cx.focus_handle(),
        }
    }

    fn delete_selected(&mut self) {
        let Some(index) = self.selected_index else {
            return;
        };

        if self.curve.control_points().len() <= 2 || index >= self.curve.control_points().len() {
            return;
        }

        let mut points = self.curve.control_points().to_vec();
        points.remove(index);
        self.curve = CubicCurve::new(points);
        self.selected_index = None;
        self.drag_index = None;
    }

    pub fn value(&self) -> &CubicCurve {
        &self.curve
    }
}

impl EventEmitter<CurveEditEvent> for CurveEditState {}

struct CurveEditPalette {
    background: Hsla,
    border: Hsla,
    grid: Hsla,
    curve: Hsla,
    control_point: Hsla,
    selected_control_point: Hsla,
}

impl CurveEditPalette {
    pub fn new(window: &Window) -> Self {
        let foreground = window.text_style().color;
        let is_dark = foreground.l > 0.5;
        let background = if is_dark {
            hsla(foreground.h, foreground.s * 0.25, 0.12, 1.)
        } else {
            hsla(foreground.h, foreground.s * 0.12, 0.94, 1.)
        };
        let accent = hsla(0.58, 0.85, if is_dark { 0.62 } else { 0.46 }, 1.);

        Self {
            background,
            border: foreground.opacity(0.28),
            grid: foreground.opacity(0.16),
            curve: foreground.opacity(0.9),
            control_point: foreground.opacity(0.78),
            selected_control_point: accent,
        }
    }
}

#[derive(Clone, Copy)]
pub struct CurveEditStyle {
    pub grid_resolution: usize,
    pub curve_resolution: usize,
    pub control_point_radius: Pixels,
    pub curve_stroke_width: Pixels,
    pub grid_stroke_width: Pixels,
    pub control_point_stroke_width: Pixels,
    pub border_width: Pixels,
}

impl Default for CurveEditStyle {
    fn default() -> Self {
        Self {
            grid_resolution: 4,
            curve_resolution: 128,
            control_point_radius: px(4.),
            curve_stroke_width: px(1.5),
            grid_stroke_width: px(0.5),
            control_point_stroke_width: px(1.),
            border_width: px(1.),
        }
    }
}

pub enum CurveEditEvent {
    ControlPointsChanged,
}

#[derive(IntoElement)]
pub struct CurveEdit {
    id: ElementId,
    style: CurveEditStyle,
    state: Entity<CurveEditState>,
    on_event: Rc<dyn Fn(CurveEditEvent, &mut Window, &mut App)>,
}

impl CurveEdit {
    pub fn new(state: &Entity<CurveEditState>) -> Self {
        Self {
            id: DEFAULT_ID.into(),
            style: CurveEditStyle::default(),
            state: state.clone(),
            on_event: Rc::new(|_, _, _| {}),
        }
    }

    pub fn id(mut self, id: impl Into<ElementId>) -> Self {
        self.id = id.into();
        self
    }

    pub fn on_event(
        mut self,
        on_event: impl Fn(CurveEditEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_event = Rc::new(on_event);
        self
    }

    pub fn curve_style(mut self, style: CurveEditStyle) -> Self {
        self.style = style;
        self
    }

    pub fn grid_resolution(mut self, resolution: usize) -> Self {
        self.style.grid_resolution = resolution;
        self
    }

    pub fn curve_resolution(mut self, resolution: usize) -> Self {
        self.style.curve_resolution = resolution.max(1);
        self
    }

    pub fn control_point_radius(mut self, radius: Pixels) -> Self {
        self.style.control_point_radius = radius;
        self
    }

    pub fn curve_stroke_width(mut self, width: Pixels) -> Self {
        self.style.curve_stroke_width = width;
        self
    }

    pub fn grid_stroke_width(mut self, width: Pixels) -> Self {
        self.style.grid_stroke_width = width;
        self
    }
}

impl RenderOnce for CurveEdit {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let state = self.state.read(cx);

        div()
            .id(self.id.clone())
            .key_context(KEY_CONTEXT)
            .track_focus(&state.focus_handle)
            .w_full()
            .h_full()
            .on_action({
                let state = self.state.clone();
                move |_: &DeleteSelectedControlPoint, window, cx| {
                    state.update(cx, |state, cx| {
                        state.delete_selected();
                        window.refresh();
                        cx.stop_propagation();
                        cx.emit(CurveEditEvent::ControlPointsChanged);
                    });
                }
            })
            .child(CurveEditCanvas {
                id: self.id,
                state: self.state.clone(),
                style: self.style,
                is_focusing: state.focus_handle.is_focused(window),
            })
    }
}

struct CurveEditCanvas {
    id: ElementId,
    state: Entity<CurveEditState>,
    style: CurveEditStyle,
    is_focusing: bool,
}

impl IntoElement for CurveEditCanvas {
    type Element = Self;
    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for CurveEditCanvas {
    type RequestLayoutState = Style;
    type PrepaintState = Bounds<Pixels>;

    fn id(&self) -> Option<ElementId> {
        Some(self.id.clone())
    }

    fn source_location(&self) -> Option<&'static Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Style) {
        let style = Style {
            size: size(relative(1.).into(), relative(1.).into()),
            ..Default::default()
        };

        let layout_id = window.request_layout(style.clone(), vec![], cx);
        (layout_id, style)
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut Style,
        window: &mut Window,
        _cx: &mut App,
    ) -> Bounds<Pixels> {
        window.with_content_mask(Some(ContentMask { bounds }), |window| {
            window.insert_hitbox(bounds, HitboxBehavior::Normal);
        });
        bounds
    }

    fn paint(
        &mut self,
        id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        _bounds: Bounds<Pixels>,
        style: &mut Style,
        bounds: &mut Bounds<Pixels>,
        window: &mut Window,
        cx: &mut App,
    ) {
        let bounds = *bounds;

        let Some(id) = id else {
            return;
        };

        let palette = CurveEditPalette::new(window);
        let paint_default_background = style.background.is_none();
        let paint_default_border = style.border_color.is_none();

        window.with_content_mask(Some(ContentMask { bounds }), |window| {
            style.paint(bounds, window, cx, |window, cx| {
                let state = self.state.read(cx);
                if paint_default_background {
                    window.paint_quad(fill(bounds, palette.background));
                }

                if paint_default_border {
                    self.paint_border(bounds, palette.border, window);
                }

                self.paint_grid(bounds, palette.grid, window);
                self.paint_curve(&state.curve, bounds, palette.curve, window);

                for (idx, &pt) in state.curve.control_points().iter().enumerate() {
                    self.paint_control_point(
                        bounds,
                        pt,
                        state.selected_index == Some(idx) && self.is_focusing,
                        &palette,
                        window,
                    );
                }
            });
        });

        self.handle_mouse_events(bounds, window);
    }
}

impl CurveEditCanvas {
    fn paint_border(&self, bounds: Bounds<Pixels>, color: Hsla, window: &mut Window) {
        let t = self.style.border_width;
        window.paint_quad(fill(
            Bounds {
                origin: bounds.origin,
                size: size(bounds.size.width, t),
            },
            color,
        ));
        window.paint_quad(fill(
            Bounds {
                origin: point(bounds.origin.x, bounds.origin.y + bounds.size.height - t),
                size: size(bounds.size.width, t),
            },
            color,
        ));
        window.paint_quad(fill(
            Bounds {
                origin: bounds.origin,
                size: size(t, bounds.size.height),
            },
            color,
        ));
        window.paint_quad(fill(
            Bounds {
                origin: point(bounds.origin.x + bounds.size.width - t, bounds.origin.y),
                size: size(t, bounds.size.height),
            },
            color,
        ));
    }

    fn paint_grid(&self, bounds: Bounds<Pixels>, color: Hsla, window: &mut Window) {
        for i in 1..self.style.grid_resolution {
            let f = i as f32 / self.style.grid_resolution as f32;
            let x = bounds.origin.x + f * bounds.size.width;
            let y = bounds.origin.y + f * bounds.size.height;
            let mut b = PathBuilder::stroke(self.style.grid_stroke_width);
            b.move_to(point(x, bounds.origin.y));
            b.line_to(point(x, bounds.origin.y + bounds.size.height));
            if let Ok(p) = b.build() {
                window.paint_path(p, color);
            }
            let mut b = PathBuilder::stroke(self.style.grid_stroke_width);
            b.move_to(point(bounds.origin.x, y));
            b.line_to(point(bounds.origin.x + bounds.size.width, y));
            if let Ok(p) = b.build() {
                window.paint_path(p, color);
            }
        }
    }

    fn paint_curve(
        &self,
        curve: &CubicCurve,
        bounds: Bounds<Pixels>,
        color: Hsla,
        window: &mut Window,
    ) {
        let sampled = curve.subdivide(self.style.curve_resolution);
        let mut b = PathBuilder::stroke(self.style.curve_stroke_width);
        b.move_to(normalized_to_screen(sampled[0], bounds));
        for &pt in &sampled[1..] {
            b.line_to(normalized_to_screen(pt, bounds));
        }
        if let Ok(p) = b.build() {
            window.paint_path(p, color);
        }
    }

    fn paint_control_point(
        &self,
        bounds: Bounds<Pixels>,
        pt: Vec2,
        selected: bool,
        palette: &CurveEditPalette,
        window: &mut Window,
    ) {
        let s = normalized_to_screen(pt, bounds);
        let r = self.style.control_point_radius;
        let bounds = Bounds {
            origin: point(s.x - r, s.y - r),
            size: size(r * 2., r * 2.),
        };
        if selected {
            window.paint_quad(fill(bounds, palette.selected_control_point));
        } else {
            window.paint_quad(PaintQuad {
                corner_radii: Corners::all(r),
                ..outline(bounds, palette.control_point, BorderStyle::Solid)
            });
        }
    }

    fn handle_mouse_events(&mut self, bounds: Bounds<Pixels>, window: &mut Window) {
        let state = self.state.clone();
        window.on_mouse_event(move |ev: &MouseDownEvent, phase, window, cx| {
            if !phase.bubble() || !bounds.contains(&ev.position) {
                return;
            }

            state.update(cx, |state, cx| {
                window.focus(&state.focus_handle, cx);

                let pick_radius = 8.0_f32;
                let closest = state
                    .curve
                    .control_points()
                    .iter()
                    .enumerate()
                    .filter_map(|(i, &p)| {
                        let p = point(
                            bounds.origin.x + p.x * bounds.size.width,
                            bounds.origin.y + (1.0 - p.y) * bounds.size.height,
                        );
                        let dx = (p.x - ev.position.x) / px(1.);
                        let dy = (p.y - ev.position.y) / px(1.);
                        let d = (dx * dx + dy * dy).sqrt();
                        (d <= pick_radius).then_some((i, d))
                    })
                    .min_by(|a, b| a.1.total_cmp(&b.1))
                    .map(|(i, _)| i);

                let index = if let Some(index) = closest {
                    index
                } else {
                    let mut control_points = state.curve.control_points().to_vec();
                    let point = screen_to_normalized(ev.position, bounds);
                    let index = control_points.partition_point(|p| p.x < point.x);
                    let min = index
                        .checked_sub(1)
                        .map_or(0., |prev| control_points[prev].x + MIN_POINT_GAP);
                    let max = control_points
                        .get(index)
                        .map_or(1., |next| next.x - MIN_POINT_GAP);
                    if min > max {
                        window.refresh();
                        return;
                    }

                    control_points.insert(
                        index,
                        Vec2::new(point.x.clamp(min, max), point.y.clamp(0., 1.)),
                    );
                    state.curve = CubicCurve::new(control_points);

                    index
                };

                state.selected_index = Some(index);
                state.drag_index = Some(index);
                window.refresh();
                cx.emit(CurveEditEvent::ControlPointsChanged);
                cx.stop_propagation();
            });
        });

        let state = self.state.clone();
        window.on_mouse_event(move |ev: &MouseMoveEvent, phase, window, cx| {
            if !phase.bubble() || !bounds.contains(&ev.position) {
                return;
            }

            state.update(cx, |state, cx| {
                let Some(idx) = state.drag_index else {
                    return;
                };

                let norm = screen_to_normalized(ev.position, bounds);
                if idx >= state.curve.control_points().len() {
                    state.drag_index = None;
                    return;
                }

                let mut control_points = state.curve.control_points().to_vec();
                let min = idx
                    .checked_sub(1)
                    .map_or(0., |prev| control_points[prev].x + MIN_POINT_GAP);
                let max = control_points
                    .get(idx + 1)
                    .map_or(1., |next| next.x - MIN_POINT_GAP);
                let x = norm.x.clamp(min.min(max), max.max(min));
                control_points[idx] = Vec2::new(x, norm.y);

                state.curve = CubicCurve::new(control_points);
                cx.emit(CurveEditEvent::ControlPointsChanged);
                cx.stop_propagation();
            });
        });

        let state = self.state.clone();
        window.on_mouse_event(move |_ev: &MouseUpEvent, _phase, window, cx| {
            state.update(cx, |state, cx| {
                if state.drag_index.take().is_some() {
                    window.refresh();
                    cx.stop_propagation();
                }
            });
        });
    }
}

fn normalized_to_screen(p: Vec2, bounds: Bounds<Pixels>) -> Point<Pixels> {
    point(
        bounds.origin.x + p.x * bounds.size.width,
        bounds.origin.y + (1.0 - p.y) * bounds.size.height,
    )
}

fn screen_to_normalized(screen: Point<Pixels>, bounds: Bounds<Pixels>) -> Vec2 {
    let x = ((screen.x - bounds.origin.x) / bounds.size.width).clamp(0., 1.);
    let y = (1.0 - (screen.y - bounds.origin.y) / bounds.size.height).clamp(0., 1.);
    Vec2::new(x, y)
}
