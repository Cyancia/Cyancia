use std::sync::Arc;

use bevy_math::{IRect, Rect};
use cyancia_color::shader::IccTransformShader;
use cyancia_image::{
    composite::{BlendFunctionRegistry, ImageCompositor, LayerPreviewOverriders},
    texel::{TexelFormat, TexelType},
    tile::{GpuTileStorage, TileStorageAppExt},
};
use cyancia_render::render_context::RenderContextAppExt;
use cyancia_tools::{ToolProxies, ToolProxyId};
use cyancia_utils::log_err::LogErr;
use glam::{IVec2, UVec2, Vec2};
use gpui::{
    BorrowAppContext, Context, DisplayId, InteractiveElement, IntoElement, MouseButton,
    MouseDownEvent, MouseMoveEvent, MouseUpEvent, ParentElement, Render, ScrollWheelEvent, Size,
    Styled, Subscription, WeakEntity, Window, canvas, div, px,
};
use moxcms::Layout;

use crate::{
    CCanvas, CanvasAppExt, CanvasId,
    control::CanvasTransform,
    event::CanvasUpdated,
    render::{CanvasRenderer, ICC_TRANSFORM_SHADER_IDENT},
};

// TODO: So, this is weird.
//       For the brush tool, when a stroke is in progress, it's preview should be generated and
//       in recomposite function, it will produce the correct output image, with the half-done
//       stroke. The problem is that, gpui is not rerendering the element. So the rendered image
//       cannot be output on time.
//       I have tried numerous ways to fix this but none of them success.
//       It's even more confusing when I found that, after commenting out the dirty rect check at
//       the beginning of recomposite function, the pan tool is still able to work correctly, while
//       the brush tool is not. All of them is recompositing the entire image on tool update.
//       So probably, wait for gpui use wgpu on windows, so we can draw the texture directly onto
//       the window surface, and that might fixes.
//
//       Edit: But it works in release build, okay nevermind.
pub struct CanvasWidget {
    tool_proxy_id: ToolProxyId,

    canvas: WeakEntity<CCanvas>,
    last_display: DisplayId,
    manage_color: bool,
    renderer: CanvasRenderer,

    output_size: UVec2,
    dirty_tiles: IRect,
    compositor: ImageCompositor,

    middle_button_drag_start: Option<(Vec2, CanvasTransform)>,

    _subscriptions: Vec<Subscription>,
}

impl CanvasWidget {
    pub fn new(
        canvas_id: CanvasId,
        tool_proxy_id: ToolProxyId,
        window: &mut Window,
        cx: &mut Context<Self>,
        manage_color: bool,
    ) -> anyhow::Result<Self> {
        let canvas_entity = cx
            .canvas(&canvas_id)
            .and_then(|e| e.upgrade())
            .ok_or_else(|| anyhow::anyhow!("Canvas {} not found.", canvas_id))?;

        let canvas = canvas_entity.read(cx);
        let device = cx.render_device();

        let icc_transform = if manage_color {
            let display_profile = cyancia_color::platform::get_window_color_profile(window)?;
            IccTransformShader::new(
                ICC_TRANSFORM_SHADER_IDENT,
                canvas.image.profile(),
                canvas.image.texel_type().moxcms_layout(),
                &display_profile,
                Layout::Rgba,
                Default::default(),
            )?
        } else {
            IccTransformShader::unmanaged(ICC_TRANSFORM_SHADER_IDENT)
        };

        let renderer = CanvasRenderer::new(
            device,
            canvas.image.texel_type(),
            // TODO probably fetch from selection layer directly?
            TexelType {
                format: TexelFormat::Alpha,
                depth: canvas.image.texel_type().depth,
            },
            &icc_transform,
        );
        let dirty_tiles = GpuTileStorage::pixel_rect_to_tile(IRect {
            min: IVec2::ZERO,
            max: canvas.image.size().as_ivec2(),
        });

        let subscription = vec![
            cx.subscribe_in(
                &canvas_entity,
                window,
                |widget, _, event: &CanvasUpdated, _, cx| {
                    widget.dirty_tiles = widget.dirty_tiles.union(event.dirty_tiles);
                    cx.notify();
                },
            ),
            cx.observe_window_bounds(window, Self::on_window_bounds_changed),
        ];

        Ok(Self {
            tool_proxy_id,
            canvas: canvas_entity.downgrade(),

            last_display: window.display(cx).unwrap().id(),
            manage_color,
            renderer,

            output_size: UVec2::ZERO,
            dirty_tiles,
            compositor: ImageCompositor::new(),

            middle_button_drag_start: None,

            _subscriptions: subscription,
        })
    }

    fn on_window_bounds_changed(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.manage_color {
            return;
        }

        let Some(display) = window.display(cx).map(|d| d.id()) else {
            return;
        };

        if display == self.last_display {
            return;
        }

        let Some(canvas) = self.canvas.upgrade().map(|e| e.read(cx)) else {
            return;
        };

        let Ok(display_profile) =
            cyancia_color::platform::get_window_color_profile(window).logged_err()
        else {
            return;
        };
        let Ok(icc_transform) = IccTransformShader::new(
            ICC_TRANSFORM_SHADER_IDENT,
            canvas.image.profile(),
            canvas.image.texel_type().moxcms_layout(),
            &display_profile,
            Layout::Rgba,
            Default::default(),
        )
        .logged_err() else {
            return;
        };

        let renderer = CanvasRenderer::new(
            cx.render_device(),
            canvas.image.texel_type(),
            // TODO probably fetch from selection layer directly?
            TexelType {
                format: TexelFormat::Alpha,
                depth: canvas.image.texel_type().depth,
            },
            &icc_transform,
        );

        self.last_display = display;
        self.renderer = renderer;
    }

    pub fn recomposite(&mut self, cx: &mut Context<Self>) {
        if self.dirty_tiles.is_empty() {
            return;
        }
        let dirty_tiles = self.dirty_tiles;
        self.dirty_tiles = IRect::EMPTY;

        cx.update_global::<LayerPreviewOverriders, _>(|overriders, cx| {
            self.canvas
                .update(cx, |canvas, cx| {
                    let tiles = cx.tile_storage();
                    let device = cx.render_device();
                    let queue = cx.render_queue();
                    let blend_funcs = BlendFunctionRegistry::global(cx);

                    self.compositor.create_cache(
                        overriders,
                        &canvas.image,
                        tiles,
                        blend_funcs,
                        device,
                        queue,
                    );
                    self.compositor.composite(
                        overriders,
                        dirty_tiles,
                        &canvas.image,
                        tiles,
                        device,
                        queue,
                    );
                })
                .ok();
        });
    }

    pub fn request_rerender(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.output_size == UVec2::ZERO {
            return;
        }

        let Some(canvas_entity) = self.canvas.upgrade() else {
            return;
        };
        self.recomposite(cx);

        let canvas = canvas_entity.read(cx);
        let tiles = cx.tile_storage();
        let device = cx.render_device().clone();
        let queue = cx.render_queue().clone();

        self.renderer
            .resize_output_buffer(&device, self.output_size);
        self.renderer.prepare(
            &device,
            &queue,
            &canvas.transform,
            canvas.image.size(),
            tiles,
            *canvas.image.layer_stack().root_id(),
            canvas.image.selection_layer(),
        );

        let tool_proxy_id = canvas.tool_proxy_id();

        self.renderer.draw(&device, &queue);
        let Some(output_texture) = self.renderer.texture() else {
            return;
        };

        cx.update_global::<ToolProxies, _>(|tool_proxies, cx| {
            tool_proxies
                .get_mut(&tool_proxy_id)
                .canvas_overlay(output_texture, window, cx);
        });
    }

    pub fn update_output_size(&mut self, size: UVec2, window: &mut Window, cx: &mut Context<Self>) {
        if self.output_size == size {
            return;
        }
        self.output_size = size;
        self.request_rerender(window, cx);
    }
}

impl Render for CanvasWidget {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tool_proxy_id = self.tool_proxy_id;
        self.request_rerender(window, cx);

        div()
            .w_full()
            .h_full()
            .overflow_hidden()
            .child(
                canvas(
                    {
                        let widget = cx.entity().downgrade();
                        move |bounds, window, cx| {
                            let _ = widget.update(cx, |this, cx| {
                                let pixels = bounds.size;
                                this.update_output_size(
                                    UVec2::new(pixels.width.into(), pixels.height.into()),
                                    window,
                                    cx,
                                );

                                let Ok(last_rect) = this
                                    .canvas
                                    .read_with(cx, |canvas, _| canvas.transform.widget_bounds)
                                else {
                                    return;
                                };

                                let min = Vec2::new(bounds.origin.x.into(), bounds.origin.y.into());
                                let max = Vec2::new(
                                    (bounds.origin.x + bounds.size.width).into(),
                                    (bounds.origin.y + bounds.size.height).into(),
                                );
                                let widget_bounds = Rect { min, max };

                                if last_rect != widget_bounds {
                                    this.canvas
                                        .update(cx, |canvas, _| {
                                            canvas.transform.widget_bounds = widget_bounds;
                                        })
                                        .ok();
                                }
                            });
                        }
                    },
                    {
                        let widget = cx.entity().downgrade();
                        let canvas = self.canvas.clone();

                        move |bounds, _, window, cx| {
                            widget
                                .read_with(cx, |widget, _| {
                                    let Some(output_texture) = widget
                                        .renderer
                                        .texture()
                                        .map(|view| view.texture().clone())
                                    else {
                                        return;
                                    };
                                    window.paint_surface(
                                        bounds,
                                        Arc::new(output_texture),
                                        Size::new(
                                            widget.output_size.x.into(),
                                            widget.output_size.y.into(),
                                        ),
                                    );
                                })
                                .ok();

                            window.on_mouse_event({
                                let widget = widget.clone();
                                move |event: &MouseMoveEvent, phase, _, cx| {
                                    if !phase.capture() {
                                        return;
                                    }

                                    widget
                                        .update(cx, |_, cx| {
                                            cx.notify();
                                        })
                                        .ok();

                                    if event
                                        .pressed_button
                                        .is_some_and(|b| b == MouseButton::Middle)
                                    {
                                        let position = Vec2::new(
                                            event.position.x.into(),
                                            event.position.y.into(),
                                        );
                                        let Ok(Some((start_position, original_transform))) = widget
                                            .read_with(cx, |w, _| {
                                                w.middle_button_drag_start.clone()
                                            })
                                        else {
                                            return;
                                        };
                                        let delta = position - start_position;
                                        canvas
                                            .update(cx, |canvas, _| {
                                                canvas.transform =
                                                    original_transform.translated(delta);
                                            })
                                            .ok();
                                    }

                                    if event.pressed_button.is_none_or(|b| b == MouseButton::Left) {
                                        cx.update_global::<ToolProxies, _>(|tool_proxies, cx| {
                                            let tool_proxy = tool_proxies.get_mut(&tool_proxy_id);
                                            tool_proxy.mouse_moved(event, cx);
                                        });
                                    }
                                }
                            });

                            window.on_mouse_event({
                                let widget = widget.clone();
                                move |event: &MouseUpEvent, phase, _, cx| {
                                    if !phase.capture() || event.button != MouseButton::Left {
                                        return;
                                    }

                                    cx.update_global::<ToolProxies, _>(|tool_proxies, cx| {
                                        let tool_proxy = tool_proxies.get_mut(&tool_proxy_id);
                                        tool_proxy.mouse_released(event, cx);
                                    });

                                    widget
                                        .update(cx, |_, cx| {
                                            cx.notify();
                                        })
                                        .ok();
                                }
                            });
                        }
                    },
                )
                .absolute()
                .size_full(),
            )
            .on_any_mouse_down(cx.listener(move |widget, event: &MouseDownEvent, _, cx| {
                match event.button {
                    MouseButton::Left => {
                        cx.update_global::<ToolProxies, _>(|tool_proxies, cx| {
                            let tool_proxy = tool_proxies.get_mut(&tool_proxy_id);
                            tool_proxy.mouse_pressed(event, cx);
                        });
                        cx.stop_propagation();
                    }
                    MouseButton::Middle => {
                        let Ok(canvas_transform) = widget
                            .canvas
                            .read_with(cx, |canvas, _| canvas.transform.clone())
                        else {
                            return;
                        };
                        widget.middle_button_drag_start = Some((
                            Vec2::new(event.position.x.into(), event.position.y.into()),
                            canvas_transform,
                        ));
                        cx.stop_propagation();
                    }
                    _ => {}
                }
                cx.notify();
            }))
            .on_scroll_wheel(cx.listener(move |widget, event: &ScrollWheelEvent, _, cx| {
                widget
                    .canvas
                    .update(cx, |canvas, _| {
                        let line_height = if event.alt { 30.0 } else { 15.0 };
                        let delta = event.delta.pixel_delta(px(line_height));
                        let delta = Vec2::new(delta.x.into(), delta.y.into());

                        if event.modifiers.control {
                            let position_ss =
                                Vec2::new(event.position.x.into(), event.position.y.into());
                            if let Some(center) = canvas.transform.window_to_pixel(position_ss) {
                                let factor = line_height / 60.0;
                                canvas.transform.scale_around(
                                    if delta.y > 0.0 {
                                        1.0 + factor
                                    } else {
                                        1.0 - factor
                                    },
                                    center,
                                );
                            }
                        } else {
                            if delta.x > 0.0 {
                                canvas.transform.translate(delta);
                            } else if event.shift {
                                canvas.transform.translate(Vec2::X * delta.y);
                            } else {
                                canvas.transform.translate(Vec2::Y * delta.y);
                            }
                        }
                    })
                    .ok();

                cx.notify();
            }))
    }
}
