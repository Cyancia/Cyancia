use std::sync::Arc;

use bevy_math::{IRect, Rect};
use cyancia_image::{
    composite::{ImageCompositor, LayerPreviewOverriders},
    texel::{TexelFormat, TexelType},
    tile::{GpuTileStorage, GpuTileStorageInner},
};
use cyancia_render::render_context::RenderContext;
use cyancia_tools::{ToolProxies, ToolProxy, ToolProxyId};
use glam::{IVec2, UVec2, Vec2};
use gpui::{
    App, AppContext, BorrowAppContext, Context, Corners, InteractiveElement, IntoElement,
    MouseButton, MouseMoveEvent, MouseUpEvent, ObjectFit, ParentElement, Render, RenderImage,
    Styled, WeakEntity, Window, canvas, div, px,
};
use wgpu::PollType;

use crate::{CCanvas, CanvasAppExt, CanvasId, event::CanvasUpdated, render::CanvasRenderer};

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
    renderer: CanvasRenderer,
    latest_image: Option<Arc<RenderImage>>,
    output_size: UVec2,
    ongoing_render: bool,
    dirty_tiles: IRect,
    compositor: ImageCompositor,
}

impl CanvasWidget {
    pub fn new(
        canvas_id: CanvasId,
        tool_proxy_id: ToolProxyId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<Self> {
        let canvas_entity = cx.canvas(&canvas_id)?.upgrade()?;
        let canvas = canvas_entity.read(cx);
        let render_context = cx.global::<RenderContext>();
        let renderer = CanvasRenderer::new(
            &render_context.device,
            canvas.image.texel_type(),
            // TODO probably fetch from selection layer directly?
            TexelType {
                format: TexelFormat::Alpha,
                depth: canvas.image.texel_type().depth,
            },
        );
        let dirty_tiles = GpuTileStorageInner::pixel_rect_to_tile(IRect {
            min: IVec2::ZERO,
            max: canvas.image.size().as_ivec2(),
        });

        cx.subscribe_in(
            &canvas_entity,
            window,
            |widget, _, event: &CanvasUpdated, _, cx| {
                widget.dirty_tiles = widget.dirty_tiles.union(event.dirty_tiles);
                cx.notify();
            },
        )
        .detach();

        Some(Self {
            tool_proxy_id,
            canvas: canvas_entity.downgrade(),
            renderer,
            latest_image: None,
            output_size: UVec2::ZERO,
            ongoing_render: false,
            dirty_tiles,
            compositor: ImageCompositor::new(),
        })
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
                    let tiles = cx.global::<GpuTileStorage>();
                    let render_context = cx.global::<RenderContext>();
                    self.compositor.create_cache(
                        overriders,
                        &canvas.image,
                        tiles,
                        &render_context.device,
                        &render_context.queue,
                    );
                    self.compositor.composite(
                        overriders,
                        dirty_tiles,
                        &canvas.image,
                        tiles,
                        &render_context.device,
                        &render_context.queue,
                    );
                })
                .ok();
        });
    }

    pub fn request_rerender(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.output_size == UVec2::ZERO {
            return;
        }

        if self.ongoing_render {
            return;
        }

        let Some(canvas_entity) = self.canvas.upgrade() else {
            return;
        };
        self.ongoing_render = true;
        self.recomposite(cx);

        let canvas = canvas_entity.read(cx);
        let render_context = cx.global::<RenderContext>();
        let tiles = cx.global::<GpuTileStorage>();

        self.renderer
            .resize_output_buffer(&render_context.device, self.output_size);
        self.renderer.prepare(
            &render_context.device,
            &render_context.queue,
            &canvas.transform,
            canvas.image.size(),
            tiles,
            canvas.image.root_id(),
            canvas.image.selection_layer(),
        );

        let device = render_context.device.clone();
        let queue = render_context.queue.clone();
        let tool_proxy_id = canvas.tool_proxy_id();

        let (submission_index, rx) = cx.update_global::<ToolProxies, _>(|tool_proxies, cx| {
            self.renderer.draw(&device, &queue, |canvas_surface| {
                tool_proxies
                    .get_mut(&tool_proxy_id)
                    .canvas_overlay(canvas_surface, window, cx);
            })
        });

        let render_task = cx.background_spawn(async move {
            device
                .poll(PollType::Wait {
                    submission_index: Some(submission_index),
                    timeout: None,
                })
                .unwrap();

            rx.await
        });

        cx.spawn(async move |this, cx| {
            let result = render_task.await;
            this.update(cx, |this, cx| {
                this.ongoing_render = false;
                if let Ok(result) = result {
                    this.latest_image = Some(result);
                }

                // log::info!("Image rendered");
                cx.notify();
            })
            .ok();
            cx.refresh();
        })
        .detach();
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
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tool_proxy_id = self.tool_proxy_id;

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
                        move |bounds, _, window, cx| {
                            if let Some(image) = widget
                                .read_with(cx, |widget, _| widget.latest_image.clone())
                                .ok()
                                .flatten()
                            {
                                let image_bounds =
                                    ObjectFit::None.get_bounds(bounds, image.size(0));
                                let _ = window.paint_image(
                                    image_bounds,
                                    Corners::all(px(0.0)),
                                    image,
                                    0,
                                    false,
                                );
                                // log::info!("Image painted");
                            }

                            window.on_mouse_event({
                                let widget = widget.clone();
                                move |event: &MouseMoveEvent, phase, window, cx| {
                                    if !phase.capture() {
                                        return;
                                    }

                                    update_tool_proxy(
                                        cx,
                                        window,
                                        &widget,
                                        tool_proxy_id,
                                        |tool_proxy, cx| {
                                            tool_proxy.mouse_moved(event, cx);
                                        },
                                    );
                                }
                            });

                            window.on_mouse_event(
                                move |event: &MouseUpEvent, phase, window, cx| {
                                    if !phase.capture() || event.button != MouseButton::Left {
                                        return;
                                    }

                                    update_tool_proxy(
                                        cx,
                                        window,
                                        &widget,
                                        tool_proxy_id,
                                        |tool_proxy, cx| {
                                            tool_proxy.mouse_released(event, cx);
                                        },
                                    );
                                },
                            );
                        }
                    },
                )
                .absolute()
                .size_full(),
            )
            .on_mouse_down(MouseButton::Left, {
                let widget = cx.entity().downgrade();
                move |event, window, cx| {
                    update_tool_proxy(cx, window, &widget, tool_proxy_id, |tool_proxy, cx| {
                        tool_proxy.mouse_pressed(event, cx);
                    });
                    cx.stop_propagation();
                }
            })
    }
}

fn update_tool_proxy(
    cx: &mut App,
    window: &mut Window,
    widget: &WeakEntity<CanvasWidget>,
    tool_proxy_id: ToolProxyId,
    f: impl FnOnce(&mut ToolProxy, &mut App),
) {
    cx.update_global::<ToolProxies, _>(|tool_proxies, cx| {
        let tool_proxy = tool_proxies.get_mut(&tool_proxy_id);
        f(tool_proxy, cx);
    });

    widget
        .update(cx, |widget, cx| {
            widget.request_rerender(window, cx);
        })
        .ok();
}
