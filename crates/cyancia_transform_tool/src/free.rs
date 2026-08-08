use bevy_math::{IRect, Rect};
use cyancia_canvas::{
    CanvasAppExt, CanvasId, CanvasUndoStackAppExt, command::TileReplaceCommand,
    control::CanvasTransform, event::CanvasUpdated,
};
use cyancia_image::{
    composite::{LayerPreviewOverriders, PixelPreviewOverrider},
    layer::LayerId,
    layer_bounds::LayerBoundsPipeline,
    scan_pixels::ScanPixelsPipeline,
    texel::TexelType,
    tile::{
        DynamicLayerStorage, GpuLayerInfo, GpuTileInfo, GpuTileStorage, LayerBinding,
        TileStorageAppExt,
    },
};
use cyancia_input::{key::KeyboardState, mouse::PressedMouseState};
use cyancia_math::rect_transform::RectTransform;
use cyancia_render::{
    bind_group_entries::BindGroupEntries,
    bind_group_layout_entries::{BindGroupLayoutEntries, binding_types},
    buffer::DynamicBuffer,
    readback::{create_readback_buffer_and_schedule_copy, readback_buffer_on_submit_async},
    render_context::RenderContextAppExt,
    util::DevicePollExt,
    wesl_jit,
};
use cyancia_runtime::{Services, event::Event};
use cyancia_tools::{ToolFunction, ToolId};
use cyancia_utils::log_err::LogErr;
use encase::ShaderType;
use glam::{Mat3, Vec2};
use iced_core::{
    Clipboard, Color, Element, Length, Point, Rectangle, Shell, Size, Theme, Vector, Widget,
    layout, mouse, renderer, widget,
};
use iced_runtime::Task;
use iced_wgpu::Renderer;
use iced_widget::{
    canvas::{Frame, Path, Stroke},
    space,
};
use wgpu::{
    BindGroupDescriptor, BindGroupLayout, BindGroupLayoutDescriptor, Buffer, BufferUsages,
    ComputePassDescriptor, ComputePipeline, ComputePipelineDescriptor, Device,
    PipelineLayoutDescriptor, Queue, ShaderModuleDescriptor, ShaderSource, ShaderStages,
    StorageTextureAccess,
};

pub struct TransformSession {
    pub canvas_id: CanvasId,
    pub target_layer_id: LayerId,
    pub selection_layer_id: LayerId,
    pub has_selection: Buffer,
    pub matrix: Mat3,
    pub translate: Vec2,
    pub rotate: f32,
    pub scale: f32,
    pub shear: f32,
    pub last_shear: Option<ShearType>,
    pub tile_bounds: IRect,
    pub pixel_bounds: IRect,
    pub result_buffer: DynamicLayerStorage,
    pub transform_pipeline: FreeTransformPipeline,
    pub ongoing_transform: Option<OngoingTransform>,
}

impl TransformSession {
    pub fn new(device: &Device, queue: &Queue, init: InitTransform) -> Self {
        Self {
            canvas_id: init.canvas_id,
            target_layer_id: init.target_layer_id,
            selection_layer_id: init.selection_layer_id,
            has_selection: init.has_selection,
            matrix: Mat3::IDENTITY,
            translate: Vec2::ZERO,
            rotate: 0.0,
            scale: 1.0,
            shear: 0.0,
            last_shear: None,
            tile_bounds: GpuTileStorage::pixel_rect_to_tile(init.pixel_bounds),
            pixel_bounds: init.pixel_bounds,
            result_buffer: DynamicLayerStorage::new(
                device.clone(),
                queue.clone(),
                GpuLayerInfo {
                    texel_type: init.target_layer_texel,
                },
            ),
            transform_pipeline: FreeTransformPipeline::new(device, init.target_layer_texel),
            ongoing_transform: None,
        }
    }

    pub fn update(&mut self, cursor_ps: Vec2) {
        let Some(ongoing) = &self.ongoing_transform else {
            return;
        };

        let frame = self.matrix;
        let delta = cursor_ps - ongoing.cursor_origin_ps;
        match ongoing.ty {
            InteractionType::Translate => {
                self.translate = ongoing.base_translate + delta;
            }
            InteractionType::Rotate(_) => {
                let center = self.translate + self.pixel_bounds_center_ps();
                let from = ongoing.cursor_origin_ps - center;
                let to = cursor_ps - center;
                self.rotate = ongoing.base_rotate + (to.y.atan2(to.x) - from.y.atan2(from.x));
            }
            InteractionType::Scale(ty) => {
                let anchor = self.translate + self.scale_anchor_ps(ty);
                let from = (ongoing.cursor_origin_ps - anchor).length();
                let to = (cursor_ps - anchor).length();
                self.scale = ongoing.base_scale * if from > f32::EPSILON { to / from } else { 1.0 };
            }
            InteractionType::Shear(ty) => {
                let (axis, extent) = match ty {
                    ShearType::Left | ShearType::Right => (
                        frame.transform_vector2(Vec2::X).normalize(),
                        frame
                            .transform_vector2(Vec2::Y * self.pixel_bounds.height() as f32)
                            .length(),
                    ),
                    ShearType::Top | ShearType::Bottom => (
                        frame.transform_vector2(Vec2::Y).normalize(),
                        frame
                            .transform_vector2(Vec2::X * self.pixel_bounds.width() as f32)
                            .length(),
                    ),
                };
                self.shear = ongoing.base_shear + delta.dot(axis) / extent.max(1.0);
                self.last_shear = Some(ty);
            }
        }
        self.update_matrix();
    }

    pub fn transformed_aabb_ps(&self) -> Rect {
        self.pixel_bounds.as_rect().transformed(&self.matrix)
    }

    pub fn quad_ps(&self) -> [Vec2; 4] {
        let b = self.pixel_bounds.as_rect();
        [
            self.matrix.transform_point2(b.min),
            self.matrix.transform_point2(Vec2::new(b.max.x, b.min.y)),
            self.matrix.transform_point2(b.max),
            self.matrix.transform_point2(Vec2::new(b.min.x, b.max.y)),
        ]
    }

    pub fn update_matrix(&mut self) {
        let pivot_ps = self.active_pivot_ps();
        let mut m = Mat3::from_translation(self.translate);
        m = m * Mat3::from_translation(pivot_ps);
        m = m * Mat3::from_angle(self.rotate);
        m = m * Mat3::from_scale(Vec2::splat(self.scale));
        if let Some(shear) = self.active_shear() {
            m = m * shear;
        }
        m = m * Mat3::from_translation(-pivot_ps);
        self.matrix = m;
    }

    fn pixel_bounds_center_ps(&self) -> Vec2 {
        let b = self.pixel_bounds;
        Vec2::new(
            (b.min.x + b.max.x) as f32 * 0.5,
            (b.min.y + b.max.y) as f32 * 0.5,
        )
    }

    fn scale_anchor_ps(&self, ty: ScaleType) -> Vec2 {
        let b = self.pixel_bounds;
        let min = b.min.as_vec2();
        let max = b.max.as_vec2();
        let cx = (b.min.x + b.max.x) as f32 * 0.5;
        let cy = (b.min.y + b.max.y) as f32 * 0.5;
        match ty {
            ScaleType::Left => Vec2::new(max.x, cy),
            ScaleType::Right => Vec2::new(min.x, cy),
            ScaleType::Top => Vec2::new(cx, max.y),
            ScaleType::Bottom => Vec2::new(cx, min.y),
            ScaleType::TopLeft => max,
            ScaleType::TopRight => Vec2::new(min.x, max.y),
            ScaleType::BottomLeft => Vec2::new(max.x, min.y),
            ScaleType::BottomRight => min,
        }
    }

    fn active_pivot_ps(&self) -> Vec2 {
        let b = self.pixel_bounds;
        let cx = (b.min.x + b.max.x) as f32 * 0.5;
        let cy = (b.min.y + b.max.y) as f32 * 0.5;
        match &self.ongoing_transform {
            Some(ongoing) => match ongoing.ty {
                InteractionType::Translate | InteractionType::Rotate(_) => Vec2::new(cx, cy),
                InteractionType::Scale(ty) => self.scale_anchor_ps(ty),
                InteractionType::Shear(ty) => match ty {
                    ShearType::Left => Vec2::new(b.max.x as f32, cy),
                    ShearType::Right => Vec2::new(b.min.x as f32, cy),
                    ShearType::Top => Vec2::new(cx, b.max.y as f32),
                    ShearType::Bottom => Vec2::new(cx, b.min.y as f32),
                },
            },
            None => Vec2::new(cx, cy),
        }
    }

    fn active_shear(&self) -> Option<Mat3> {
        match self.last_shear {
            Some(ShearType::Left | ShearType::Right) => Some(Mat3::from_cols_array(&[
                1.0, 0.0, 0.0, self.shear, 1.0, 0.0, 0.0, 0.0, 1.0,
            ])),
            Some(ShearType::Top | ShearType::Bottom) => Some(Mat3::from_cols_array(&[
                1.0, self.shear, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0,
            ])),
            None => None,
        }
    }
}

pub struct OngoingTransform {
    pub cursor_origin_ps: Vec2,
    pub ty: InteractionType,
    pub base_translate: Vec2,
    pub base_rotate: f32,
    pub base_scale: f32,
    pub base_shear: f32,
}

pub fn hit_test(quad: [Vec2; 4], p: Vec2) -> Option<InteractionType> {
    let [tl, tr, br, bl] = quad;
    let x = tr - tl;
    let y = bl - tl;
    let det = x.x * y.y - x.y * y.x;
    if det.abs() < f32::EPSILON {
        return None;
    }

    let d = p - tl;
    let u = (d.x * y.y - d.y * y.x) / det;
    let v = (x.x * d.y - x.y * d.x) / det;
    let inside = u >= 0.0 && u <= 1.0 && v >= 0.0 && v <= 1.0;
    let closest = tl + x * u.clamp(0.0, 1.0) + y * v.clamp(0.0, 1.0);
    if closest.distance(p) > 20.0 && !inside {
        return None;
    }

    let x_hat = x.normalize();
    let y_hat = y.normalize();
    let w_abs = x.length();
    let h_abs = y.length();
    let center = (tl + tr + br + bl) * 0.25;

    let dl = (p - tl).dot(x_hat);
    let dr = (p - tr).dot(x_hat);
    let dt = (p - tl).dot(y_hat);
    let db = (p - bl).dot(y_hat);
    if dl > 10.0 && dr < -10.0 && dt > 10.0 && db < -10.0 {
        return Some(InteractionType::Translate);
    }

    if p.distance(tl) < 10.0 {
        return Some(InteractionType::Scale(ScaleType::TopLeft));
    }
    if p.distance(br) < 10.0 {
        return Some(InteractionType::Scale(ScaleType::BottomRight));
    }
    if p.distance(tr) < 10.0 {
        return Some(InteractionType::Scale(ScaleType::TopRight));
    }
    if p.distance(bl) < 10.0 {
        return Some(InteractionType::Scale(ScaleType::BottomLeft));
    }

    if (p - center).dot(y_hat).abs() < h_abs.min(10.0) {
        if dl.abs() < 10.0 {
            return Some(InteractionType::Shear(ShearType::Left));
        }
        if dr.abs() < 10.0 {
            return Some(InteractionType::Shear(ShearType::Right));
        }
    }

    if (p - center).dot(x_hat).abs() < w_abs.min(10.0) {
        if dt.abs() < 10.0 {
            return Some(InteractionType::Shear(ShearType::Top));
        }
        if db.abs() < 10.0 {
            return Some(InteractionType::Shear(ShearType::Bottom));
        }
    }

    if u < 0.0 {
        if v < 0.0 {
            return Some(InteractionType::Rotate(RotateType::TopLeft));
        }
        if v > 1.0 {
            return Some(InteractionType::Rotate(RotateType::BottomLeft));
        }
    }

    if u > 1.0 {
        if v < 0.0 {
            return Some(InteractionType::Rotate(RotateType::TopRight));
        }
        if v > 1.0 {
            return Some(InteractionType::Rotate(RotateType::BottomRight));
        }
    }

    None
}

#[derive(Debug, Clone, Copy)]
pub enum InteractionType {
    Translate,
    Rotate(RotateType),
    Scale(ScaleType),
    Shear(ShearType),
}

#[derive(Debug, Clone, Copy)]
pub enum RotateType {
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

#[derive(Debug, Clone, Copy)]
pub enum ScaleType {
    Left,
    Right,
    Top,
    Bottom,
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

#[derive(Debug, Clone, Copy)]
pub enum ShearType {
    Left,
    Right,
    Top,
    Bottom,
}

pub struct InitTransform {
    pub canvas_id: CanvasId,
    pub target_layer_id: LayerId,
    pub selection_layer_id: LayerId,
    pub has_selection: Buffer,
    pub target_layer_texel: TexelType,
    pub pixel_bounds: IRect,
}

pub enum FreeTransformToolMessage {
    InitTransform(InitTransform),
}

#[derive(Default)]
pub struct FreeTransformTool {
    session: Option<TransformSession>,
    bounds_cache: Option<(TexelType, LayerBoundsPipeline)>,
    scan_pipeline: Option<ScanPixelsPipeline>,
}

impl ToolFunction for FreeTransformTool {
    type Message = FreeTransformToolMessage;

    fn id() -> ToolId {
        ToolId::new("free_transform_tool".into())
    }

    fn activate(&mut self, services: &mut Services) -> Task<Self::Message> {
        let Some(canvas) = services.current_canvas() else {
            return Task::none();
        };

        // TODO change session if active layer is changed
        let canvas_id = canvas.id();
        let target_layer_id = canvas.active_layer_id();
        let selection_layer_id = canvas.image.selection_layer();
        let target_layer_texel = {
            let tiles = services.tile_storage();
            let target_layer = tiles.get_layer(target_layer_id).unwrap();
            target_layer.layer_info().texel_type
        };

        let device = services.render_device();
        let queue = services.render_queue();

        if self.bounds_cache.as_ref().map(|(t, _)| *t) != Some(target_layer_texel) {
            self.bounds_cache = Some((
                target_layer_texel,
                LayerBoundsPipeline::new(device, target_layer_texel, true),
            ));
        }
        let bounds_pipeline = &self.bounds_cache.as_ref().unwrap().1;
        if self.scan_pipeline.is_none() {
            self.scan_pipeline = Some(ScanPixelsPipeline::new(device, TexelType::A8));
        }
        let scan_pipeline = self.scan_pipeline.as_ref().unwrap();

        let tiles = services.tile_storage();
        let target_layer = tiles.get_layer_binding_or_empty(target_layer_id).unwrap();
        let selection_binding = tiles
            .get_layer_binding_or_empty(selection_layer_id)
            .unwrap();
        let mut has_selection =
            Some(scan_pipeline.scan_to_binary_buffer(device, queue, &selection_binding));

        let mut ec = device.create_command_encoder(&Default::default());
        let bounds_buffer = bounds_pipeline.dispatch(
            device,
            queue,
            &mut ec,
            target_layer,
            Some(selection_binding),
        );
        let bounds_buffer_staging =
            create_readback_buffer_and_schedule_copy(device, &mut ec, &bounds_buffer);
        let bounds_buffer_readback =
            readback_buffer_on_submit_async::<IRect, _>(&mut ec, &bounds_buffer_staging, ..)
                .into_task();
        let si = queue.submit([ec.finish()]);
        let poll_task = Task::future({
            let device = device.clone();
            async move {
                let _ = device.poll_indefinitely_for(si);
            }
        });

        let init_task = bounds_buffer_readback.then(move |m| match m {
            Ok(bounds) => {
                let has_selection = has_selection.take().unwrap();
                Task::done(FreeTransformToolMessage::InitTransform(InitTransform {
                    canvas_id,
                    target_layer_id,
                    selection_layer_id,
                    has_selection,
                    target_layer_texel,
                    pixel_bounds: bounds,
                }))
            }
            _ => Task::none(),
        });

        Task::batch([init_task, poll_task.discard()])
    }

    fn begin(
        &mut self,
        _keyboard: &KeyboardState,
        mouse: &PressedMouseState,
        services: &mut Services,
    ) -> Task<Self::Message> {
        let Some(session) = &mut self.session else {
            return Task::none();
        };
        let Some(canvas) = services.canvas(&session.canvas_id) else {
            return Task::none();
        };
        let Some(cursor_ps) = canvas
            .transform
            .window_to_pixel(Vec2::new(mouse.position.x, mouse.position.y))
        else {
            return Task::none();
        };
        let Some(ty) = hit_test(session.quad_ps(), cursor_ps) else {
            return Task::none();
        };

        session.ongoing_transform = Some(OngoingTransform {
            cursor_origin_ps: cursor_ps,
            ty: ty,
            base_translate: session.translate,
            base_rotate: session.rotate,
            base_scale: session.scale,
            base_shear: session.shear,
        });

        Task::none()
    }

    fn update(
        &mut self,
        _keyboard: &KeyboardState,
        mouse: &PressedMouseState,
        services: &mut Services,
    ) -> Task<Self::Message> {
        let Some(session) = &mut self.session else {
            return Task::none();
        };
        let Some(canvas) = services.canvas(&session.canvas_id) else {
            return Task::none();
        };
        let Some(cursor_ps) = canvas
            .transform
            .window_to_pixel(Vec2::new(mouse.position.x, mouse.position.y))
        else {
            return Task::none();
        };

        if session.ongoing_transform.is_none() {
            return Task::none();
        }

        session.update(cursor_ps);

        session.result_buffer.allocate_tiles(session.tile_bounds);
        let transformed_tiles =
            GpuTileStorage::pixel_rect_to_tile(session.transformed_aabb_ps().as_irect());
        session.result_buffer.allocate_tiles(transformed_tiles);

        let device = services.render_device();
        let queue = services.render_queue();
        let tiles = services.tile_storage();
        let target_layer = tiles
            .get_layer_binding_or_empty(session.target_layer_id)
            .unwrap();
        let selection_layer = tiles
            .get_layer_binding_or_empty(session.selection_layer_id)
            .unwrap();

        session.transform_pipeline.dispatch(
            device,
            queue,
            &FreeTransformParams {
                mat_inv: session.matrix.inverse(),
            },
            target_layer,
            session.result_buffer.binding_or_empty(),
            selection_layer,
            &session.has_selection,
        );

        let overriders = services.service_mut::<LayerPreviewOverriders>();
        overriders.insert_overrider(
            session.target_layer_id,
            PixelPreviewOverrider::from_layer_storage(&session.result_buffer),
        );
        CanvasUpdated::broadcast(CanvasUpdated {
            id: session.canvas_id,
            dirty_tiles: transformed_tiles.union(session.tile_bounds),
        });

        Task::none()
    }

    fn end(
        &mut self,
        _keyboard: &KeyboardState,
        mouse: &PressedMouseState,
        services: &mut Services,
    ) -> Task<Self::Message> {
        let Some(session) = &mut self.session else {
            return Task::none();
        };

        session.ongoing_transform = None;
        Task::none()
    }

    fn handle_message(
        &mut self,
        message: Self::Message,
        services: &mut Services,
    ) -> Task<Self::Message> {
        match message {
            FreeTransformToolMessage::InitTransform(init) => {
                let device = services.render_device();
                let queue = services.render_queue();
                self.session = Some(TransformSession::new(device, queue, init));
                Task::none()
            }
        }
    }

    fn deactivate(&mut self, services: &mut Services) -> Task<Self::Message> {
        let Some(session) = self.session.take() else {
            return Task::none();
        };
        commit_transform(session, services);

        Task::none()
    }

    fn canvas_overlay<'a>(
        &'a self,
        services: &'a Services,
    ) -> Element<'a, Self::Message, Theme, Renderer> {
        let Some(session) = &self.session else {
            return space().into();
        };
        let Some(canvas) = services.current_canvas() else {
            return space().into();
        };

        Element::new(FreeTransformToolOverlay {
            canvas_transform: &canvas.transform,
            session,
        })
    }
}

fn commit_transform(session: TransformSession, services: &mut Services) {
    let Some(result_texture) = session.result_buffer.texture() else {
        return;
    };

    services
        .service_mut::<LayerPreviewOverriders>()
        .remove_overrider(&session.target_layer_id);

    let tiles = services.tile_storage();
    let target_layer = tiles.get_layer(session.target_layer_id).unwrap();
    let cmd = TileReplaceCommand::new(
        "Free Transform".into(),
        session.canvas_id,
        services.render_device(),
        services.render_queue(),
        session.target_layer_id,
        &target_layer,
        session.result_buffer.iter_tile_indices().collect(),
        result_texture.clone(),
    );
    drop(target_layer);

    services
        .push_undo_command(&session.canvas_id, cmd)
        .log_err();
}

pub struct FreeTransformToolOverlay<'a> {
    pub canvas_transform: &'a CanvasTransform,
    pub session: &'a TransformSession,
}

impl<'a> Widget<FreeTransformToolMessage, Theme, Renderer> for FreeTransformToolOverlay<'a> {
    fn size(&self) -> Size<Length> {
        Size::new(Length::Fill, Length::Fill)
    }

    fn layout(
        &mut self,
        _tree: &mut widget::Tree,
        _renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        layout::atomic(limits, Length::Fill, Length::Fill)
    }

    fn draw(
        &self,
        _tree: &widget::Tree,
        renderer: &mut Renderer,
        _theme: &Theme,
        _style: &renderer::Style,
        layout: layout::Layout<'_>,
        _cursor: mouse::Cursor,
        _viewport: &Rectangle,
    ) {
        let corners = self
            .session
            .quad_ps()
            .map(|p| self.canvas_transform.pixel_to_window(p));
        let [top_left, top_right, bottom_right, bottom_left] = corners;

        let mut frame = Frame::new(renderer, layout.bounds().size());

        for (color, translation) in [
            (Color::WHITE, Vector::ZERO),
            (Color::BLACK, Vector::new(1.0, 1.0)),
        ] {
            frame.push_transform();
            frame.translate(translation);

            const HANDLE_SIZE: f32 = 10.0;
            for point in [top_left, top_right, bottom_left, bottom_right] {
                let origin = Point::new(point.x - HANDLE_SIZE * 0.5, point.y - HANDLE_SIZE * 0.5);
                frame.stroke_rectangle(
                    origin,
                    Size::new(HANDLE_SIZE, HANDLE_SIZE),
                    Stroke {
                        style: color.into(),
                        width: 1.0,
                        ..Default::default()
                    },
                );
            }

            let path = Path::new(|b| {
                let hx = Vec2::X * HANDLE_SIZE;
                let hy = Vec2::Y * HANDLE_SIZE;

                for seg in [
                    (top_left + hx, top_right - hx),
                    (top_right + hy, bottom_right - hy),
                    (bottom_right - hx, bottom_left + hx),
                    (bottom_left - hy, top_left + hy),
                ] {
                    let (p1, p2) = seg;
                    b.move_to(Point::new(p1.x, p1.y));
                    b.line_to(Point::new(p2.x, p2.y));
                }
            });
            frame.stroke(
                &path,
                Stroke {
                    style: color.into(),
                    width: 1.0,
                    ..Default::default()
                },
            );

            frame.pop_transform();
        }

        iced_graphics::geometry::Renderer::draw_geometry(renderer, frame.into_geometry());
    }

    fn mouse_interaction(
        &self,
        _tree: &widget::Tree,
        _layout: layout::Layout<'_>,
        cursor: mouse::Cursor,
        _viewport: &Rectangle,
        _renderer: &Renderer,
    ) -> mouse::Interaction {
        let Some(cursor_ps) = cursor
            .position()
            .and_then(|p| self.canvas_transform.window_to_pixel(Vec2::new(p.x, p.y)))
        else {
            return mouse::Interaction::None;
        };

        let Some(ty) = self
            .session
            .ongoing_transform
            .as_ref()
            .map(|t| t.ty)
            .or_else(|| hit_test(self.session.quad_ps(), cursor_ps))
        else {
            return mouse::Interaction::None;
        };

        dbg!(ty);

        match ty {
            InteractionType::Translate => mouse::Interaction::Move,
            // TODO
            InteractionType::Rotate(_ty) => mouse::Interaction::None,
            InteractionType::Scale(ty) => match ty {
                ScaleType::Left | ScaleType::Right => mouse::Interaction::ResizingRow,
                ScaleType::Top | ScaleType::Bottom => mouse::Interaction::ResizingColumn,
                ScaleType::TopLeft | ScaleType::BottomRight => {
                    mouse::Interaction::ResizingDiagonallyDown
                }
                ScaleType::TopRight | ScaleType::BottomLeft => {
                    mouse::Interaction::ResizingDiagonallyUp
                }
            },
            // TODO
            InteractionType::Shear(_ty) => mouse::Interaction::None,
        }
    }
}

pub struct FreeTransformPipeline {
    layout: BindGroupLayout,
    pipeline: ComputePipeline,
}

impl FreeTransformPipeline {
    pub fn new(device: &Device, format: TexelType) -> Self {
        let shader = wesl_jit::compile_wesl_with_config(
            include_str!("free.wesl").into(),
            &[&cyancia_image::image::PACKAGE],
            |compiler| {
                compiler.set_feature(format.shader_def(), true);
            },
        )
        .unwrap();

        let layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some("free transform bind group layout"),
            entries: BindGroupLayoutEntries::sequential(
                ShaderStages::COMPUTE,
                (
                    binding_types::texture_storage_2d_array(
                        format.wgpu_format(),
                        StorageTextureAccess::ReadOnly,
                    ),
                    binding_types::texture_storage_2d_array(
                        format.wgpu_format(),
                        StorageTextureAccess::WriteOnly,
                    ),
                    binding_types::storage_buffer_read_only::<GpuTileInfo>(false),
                    binding_types::storage_buffer_read_only::<GpuTileInfo>(false),
                    binding_types::texture_storage_2d_array(
                        wgpu::TextureFormat::R8Unorm,
                        StorageTextureAccess::ReadOnly,
                    ),
                    binding_types::storage_buffer_read_only::<GpuTileInfo>(false),
                    binding_types::storage_buffer_read_only::<u32>(false),
                    binding_types::storage_buffer_read_only::<FreeTransformParams>(false),
                ),
            )
            .as_ref(),
        });

        let pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: Some("free transform pipeline layout"),
            bind_group_layouts: &[&layout],
            ..Default::default()
        });

        let module = device.create_shader_module(ShaderModuleDescriptor {
            label: Some("free transform shader module"),
            source: ShaderSource::Wgsl(shader.into()),
        });

        let pipeline = device.create_compute_pipeline(&ComputePipelineDescriptor {
            label: Some("free transform pipeline"),
            layout: Some(&pipeline_layout),
            module: &module,
            entry_point: Some("main"),
            compilation_options: Default::default(),
            cache: None,
        });

        Self { layout, pipeline }
    }

    pub fn dispatch(
        &self,
        device: &Device,
        queue: &Queue,
        params: &FreeTransformParams,
        layer: LayerBinding,
        output: LayerBinding,
        selection: LayerBinding,
        has_selection: &Buffer,
    ) {
        let mut params_buffer = DynamicBuffer::new(
            Some("free_transform_params_buffer".into()),
            BufferUsages::STORAGE,
        );
        params_buffer.push(params);
        params_buffer.write_buffer(device, queue);

        let bind_group = device.create_bind_group(&BindGroupDescriptor {
            label: Some("free transform bind group"),
            layout: &self.layout,
            entries: BindGroupEntries::sequential((
                &layer.texture,
                &output.texture,
                layer.tile_info_buffer.as_entire_binding(),
                output.tile_info_buffer.as_entire_binding(),
                &selection.texture,
                selection.tile_info_buffer.as_entire_binding(),
                has_selection.as_entire_binding(),
                params_buffer.binding().unwrap(),
            ))
            .as_ref(),
        });

        let mut ec = device.create_command_encoder(&Default::default());
        {
            let mut pass = ec.begin_compute_pass(&ComputePassDescriptor {
                label: Some("free transform pass"),
                ..Default::default()
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &bind_group, &[]);
            pass.dispatch_workgroups(
                GpuTileStorage::TILE_SIZE.div_ceil(16),
                GpuTileStorage::TILE_SIZE.div_ceil(16),
                output.texture.texture().depth_or_array_layers(),
            );
        }
        queue.submit([ec.finish()]);
    }
}

#[derive(ShaderType, Debug)]
pub struct FreeTransformParams {
    pub mat_inv: Mat3,
}
