use std::sync::Arc;

use bevy_math::{IRect, Rect};
use cyancia_image::{
    composite::{ImageCompositor, LayerPreviewOverriders},
    texel::TexelType,
    tile::{GpuTileStorage, GpuTileStorageInner},
};
use cyancia_render::render_context::RenderContext;
use cyancia_tools::{ToolProxies, ToolProxy, ToolProxyId};
use glam::{IVec2, UVec2, Vec2};
use gpui::{
    App, AppContext, BorrowAppContext, Context, InteractiveElement, IntoElement, MouseButton,
    MouseMoveEvent, MouseUpEvent, ObjectFit, ParentElement, Render, RenderImage, RenderOnce,
    Styled, StyledImage, WeakEntity, Window, canvas, div, img, prelude::FluentBuilder, px,
};
use gpui_component::ElementExt;
use wgpu::{Device, PollType};

use crate::{
    CCanvas, CanvasAppExt, CanvasId, CanvasManager,
    event::CanvasUpdated,
    render::{CanvasRenderer, CanvasUniform},
};

pub struct CanvasWidget {
    canvas_id: CanvasId,
    tool_proxy_id: ToolProxyId,
    canvas: WeakEntity<CCanvas>,
    renderer: CanvasRenderer,
    latest_image: Option<Arc<RenderImage>>,
    output_size: UVec2,
    ongoing_render: bool,
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
        let renderer = CanvasRenderer::new(&render_context.device, canvas.image.texel_type());

        cx.subscribe_in(
            &canvas_entity,
            window,
            |widget, canvas, event: &CanvasUpdated, window, cx| {
                widget.recomposite(cx, Some(event.dirty_tiles));
            },
        )
        .detach();

        Some(Self {
            canvas_id,
            tool_proxy_id,
            canvas: canvas_entity.downgrade(),
            renderer,
            latest_image: None,
            output_size: UVec2::ZERO,
            ongoing_render: false,
            compositor: ImageCompositor::new(),
        })
    }

    pub fn recomposite(&mut self, cx: &mut Context<Self>, dirty_tiles: Option<IRect>) {
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
                        dirty_tiles.unwrap_or_else(|| {
                            GpuTileStorageInner::pixel_rect_to_tile(IRect {
                                min: IVec2::ZERO,
                                max: canvas.image.size().as_ivec2(),
                            })
                        }),
                        &canvas.image,
                        tiles,
                        &render_context.device,
                        &render_context.queue,
                    );
                })
                .ok();
        });
    }

    pub fn rerender(&mut self, cx: &mut Context<Self>) {
        if self.ongoing_render {
            return;
        }

        let Some(canvas) = self.canvas.upgrade().map(|c| c.read(cx)) else {
            return;
        };
        if self.output_size == UVec2::ZERO {
            return;
        }

        self.ongoing_render = true;

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
        );
        let (submission_index, rx) = self
            .renderer
            .draw(&render_context.device, &render_context.queue);

        let device = render_context.device.clone();
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
            let _ = this.update(cx, |this, cx| {
                this.ongoing_render = false;
                if let Ok(result) = result {
                    this.latest_image = Some(result);
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub fn update_output_size(&mut self, size: UVec2, cx: &mut Context<Self>) {
        if self.output_size == size {
            return;
        }
        self.output_size = size;
        self.rerender(cx);
    }
}

impl Render for CanvasWidget {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tool_proxy_id = self.tool_proxy_id;

        div()
            .w_full()
            .h_full()
            .on_prepaint({
                let this = cx.entity().downgrade();
                move |bounds, window, cx| {
                    let _ = this.update(cx, |this, cx| {
                        let pixels = bounds.size;
                        this.update_output_size(
                            UVec2::new(pixels.width.into(), pixels.height.into()),
                            cx,
                        );

                        let Ok(last_rect) = this
                            .canvas
                            .read_with(cx, |canvas, cx| canvas.transform.widget_bounds)
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
                                .update(cx, |canvas, cx| {
                                    canvas.transform.widget_bounds = widget_bounds;
                                })
                                .ok();
                        }
                    });
                }
            })
            .when_some(self.latest_image.clone(), |d, i| {
                d.child(
                    img(i)
                        .w_full()
                        .h_full()
                        .overflow_hidden()
                        .object_fit(ObjectFit::None),
                )
            })
            .child(
                canvas(|_, _, _| {}, {
                    let widget = cx.entity().downgrade();
                    move |_, _, window, cx| {
                        window.on_mouse_event({
                            let widget = widget.clone();
                            move |event: &MouseMoveEvent, phase, window, cx| {
                                if !phase.capture() {
                                    return;
                                }

                                update_tool_proxy(cx, &widget, tool_proxy_id, |tool_proxy, cx| {
                                    tool_proxy.mouse_moved(event, cx);
                                });
                            }
                        });

                        window.on_mouse_event(move |event: &MouseUpEvent, phase, window, cx| {
                            if !phase.capture() || event.button != MouseButton::Left {
                                return;
                            }

                            update_tool_proxy(cx, &widget, tool_proxy_id, |tool_proxy, cx| {
                                tool_proxy.mouse_released(event, cx);
                            });
                        });
                    }
                })
                .absolute()
                .size_full(),
            )
            .on_mouse_down(MouseButton::Left, {
                let widget = cx.entity().downgrade();
                move |event, window, cx| {
                    update_tool_proxy(cx, &widget, tool_proxy_id, |tool_proxy, cx| {
                        tool_proxy.mouse_pressed(event, cx);
                    });
                    cx.stop_propagation();
                }
            })
    }
}

fn update_tool_proxy(
    cx: &mut App,
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
            widget.rerender(cx);
        })
        .ok();
}
