use std::{any::Any, fmt::Debug, sync::Arc};

use cyancia_brush::tool::BrushTool;
use cyancia_canvas::{
    CCanvas, CanvasAppExt, CanvasId, CanvasManager, GlobalCanvasEvents,
    event::{CanvasCreated, CanvasRemoved},
    render::CanvasRenderer,
};
use cyancia_theme::{SwitchThemeAction, ThemeAsset};
use cyancia_tools::{ToolFunction, ToolId, ToolProxies, ToolProxy};
use cyancia_view::{View, ViewId};
use glam::UVec2;
use gpui::{
    App, AppContext, BorrowAppContext, Context, Entity, FocusHandle, InteractiveElement,
    IntoElement, Menu, MenuItem, ParentElement, Render, Styled, WeakEntity, Window, WindowHandle,
    WindowOptions, div,
};
use gpui_component::{
    ActiveTheme, GlobalState, Root, Theme, ThemeRegistry, TitleBar,
    dock::{DockArea, DockItem, DockPlacement, DockState, PanelView},
    menu::AppMenuBar,
};
use parking_lot::Mutex;
use uuid::Uuid;

use crate::dock::{CanvasDock, CurrentCanvasLayersDock, FiltersDock, LayersDock, ToolOptionsDock};

fn default_dock_layout(
    dock_area: &WeakEntity<DockArea>,
    window: &mut Window,
    cx: &mut App,
) -> DockItem {
    DockItem::h_split(
        vec![DockItem::tabs(
            vec![
                Arc::new(cx.new(LayersDock::new)),
                Arc::new(cx.new(FiltersDock::new)),
                Arc::new(cx.new(|cx| ToolOptionsDock::new(window, cx))),
                Arc::new(cx.new(|cx| CurrentCanvasLayersDock::new(window, cx))),
            ],
            dock_area,
            window,
            cx,
        )],
        dock_area,
        window,
        cx,
    )
}

pub const MAIN_VIEW_CONTEXT: &'static str = "main_view";

pub struct MainView {
    menu_bar: Entity<AppMenuBar>,
    dock_area: Entity<DockArea>,
    focus_handle: FocusHandle,
}

impl View for MainView {
    fn id() -> ViewId {
        ViewId::new("main_view")
    }

    fn open(cx: &mut App) -> anyhow::Result<WindowHandle<Root>> {
        cx.open_window(
            WindowOptions {
                titlebar: None,
                ..Default::default()
            },
            |window, cx| {
                let main_view = cx.new(|cx| MainView::new(window, cx));
                let root_view = cx.new(|cx| Root::new(main_view, window, cx));
                root_view
            },
        )
    }
}

impl MainView {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let dock_area_entity = cx.new(|cx| DockArea::new("main-dock-area", None, window, cx));

        dock_area_entity.update(cx, |dock_area, cx| {
            dock_area.set_center(
                default_dock_layout(&dock_area_entity.downgrade(), window, cx),
                window,
                cx,
            );
        });

        let menu_bar = AppMenuBar::new(cx);
        update_menu_bar(&menu_bar, cx);
        cx.observe_global::<Theme>({
            let menu_bar = menu_bar.clone();
            move |theme, cx| {
                update_menu_bar(&menu_bar, cx);
            }
        })
        .detach();

        let canvas_events = cx.global_canvas_events_entity();
        cx.subscribe_in(&canvas_events, window, Self::on_canvas_created)
            .detach();
        let focus_handle = cx.focus_handle();
        focus_handle.focus(window, cx);

        Self {
            menu_bar,
            dock_area: dock_area_entity,
            focus_handle,
        }
    }

    fn on_canvas_created(
        &mut self,
        _: &Entity<GlobalCanvasEvents>,
        event: &CanvasCreated,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(canvas) = cx.read_canvas(&event.id) else {
            return;
        };
        let canvas_id = canvas.id();
        let tool_proxy_id = canvas.tool_proxy_id();

        cx.update_global::<ToolProxies, _>(|tool_proxies, cx| {
            tool_proxies
                .get_mut(&tool_proxy_id)
                .switch_tool(BrushTool::id(), cx);
        });

        self.dock_area.update(cx, |dock_area, cx| {
            dock_area.add_panel(
                Arc::new(cx.new(|cx| CanvasDock::new(canvas_id, tool_proxy_id, window, cx))),
                DockPlacement::Center,
                None,
                window,
                cx,
            );
        });
    }
}

fn update_menu_bar(menu_bar: &Entity<AppMenuBar>, cx: &mut App) {
    cx.set_menus(build_menu_bar(cx));
    let menus = build_menu_bar(cx).into_iter().map(|m| m.owned()).collect();
    GlobalState::global_mut(cx).set_app_menus(menus);
    menu_bar.update(cx, |menu_bar, cx| {
        menu_bar.reload(cx);
    });
}

fn build_menu_bar(cx: &App) -> Vec<Menu> {
    let current_theme = cx.theme().theme_name();
    let themes = ThemeRegistry::global(cx)
        .sorted_themes()
        .into_iter()
        .map(|theme| {
            MenuItem::action(
                theme.name.clone(),
                SwitchThemeAction {
                    theme: theme.name.clone(),
                },
            )
            .checked(&theme.name == current_theme)
        })
        .collect::<Vec<_>>();

    vec![Menu {
        name: "Window".into(),
        items: vec![MenuItem::Submenu(Menu {
            name: "Themes".into(),
            items: themes,
            disabled: false,
        })],
        disabled: false,
    }]
}

impl Render for MainView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .track_focus(&self.focus_handle)
            .key_context(MAIN_VIEW_CONTEXT)
            .w_full()
            .h_full()
            .child(TitleBar::new().child(self.menu_bar.clone()))
            .child(self.dock_area.clone())
    }
}
