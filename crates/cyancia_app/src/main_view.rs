use std::{any::Any, fmt::Debug, sync::Arc};

use cyancia_brush::tool::BrushTool;
use cyancia_canvas::{
    CCanvas, CanvasEvents, CanvasId, CanvasManager,
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

use crate::dock::{CanvasDock, FiltersDock, LayersDock, ToolOptionsDock};

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
                Arc::new(cx.new(ToolOptionsDock::new)),
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

        let canvas_events = cx.global::<CanvasManager>().events().clone();
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
        _: &Entity<CanvasEvents>,
        event: &CanvasCreated,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let canvas_manager = cx.global_mut::<CanvasManager>();
        let Some(canvas) = canvas_manager.get(&event.id) else {
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

// impl WindowView for MainView {
//     type Message = MainViewMessage;

//     fn id() -> WindowViewId {
//         WindowViewId::new("main_view")
//     }

//     fn boot(services: &mut Services) -> (Self, Task<Self::Message>) {
//         let actions = services
//             .service::<ActionManifestCollection>()
//             .subset_for_view("main_view");
//         let actions_matcher = Arc::new(Mutex::new(ActionsMatcher::new(actions)));

//         let img = CImage::new(UVec2 { x: 1024, y: 768 });
//         let root_layer = img.root_id();
//         let canvas = CCanvas::new(
//             img,
//             services.service_mut::<ToolProxies>().add(ToolProxy::new()),
//         );
//         // TODO this should not be done here
//         let tiles = services.service::<GpuTileStorage>();
//         for layer in canvas.image.layer_stack().iter_layers() {
//             tiles.declare_layer(
//                 layer.id(),
//                 GpuLayerInfo {
//                     // TODO
//                     texel_type: TexelType::RGBA8,
//                 },
//             );
//         }

//         let (main_window, task) = window::open(Default::default());
//         let (mut dock_manager, dock_manager_task) = DockManager::new(main_window);
//         let canvas_dock = CanvasDock::new(canvas.id(), actions_matcher.clone());
//         let canvas_dock_id = <CanvasDock as Dock<Theme, Renderer>>::id(&canvas_dock);
//         let current_canvas_layers_dock = CurrentCanvasLayersDock::new();
//         let current_canvas_layers_dock_id =
//             <CurrentCanvasLayersDock as Dock<Theme, Renderer>>::id(&current_canvas_layers_dock);
//         dock_manager.register_dock(canvas_dock);
//         dock_manager.register_dock(current_canvas_layers_dock);
//         dock_manager.register_dock(crate::dock::LayerDock);
//         dock_manager.register_dock(crate::dock::ToolDock);
//         dock_manager.register_dock(crate::dock::HistoryDock);

//         let dock_tasks = Task::batch([
//             dock_manager.open_dock(canvas_dock_id),
//             dock_manager.open_dock(current_canvas_layers_dock_id),
//             dock_manager.open_dock(DockId::new(crate::dock::LAYER_DOCK_ID.into())),
//             dock_manager.open_dock(DockId::new(crate::dock::TOOL_DOCK_ID.into())),
//             dock_manager.open_dock(DockId::new(crate::dock::HISTORY_DOCK_ID.into())),
//         ])
//         .map(MainViewMessage::Dock);

//         services.service_mut::<CanvasManager>().add_canvas(canvas);

//         (
//             Self {
//                 dock_manager,
//                 actions_matcher,
//             },
//             Task::batch([
//                 task.discard(),
//                 dock_manager_task.map(MainViewMessage::Dock),
//                 dock_tasks,
//             ]),
//         )
//     }

//     fn view<'a>(
//         &'a self,
//         window: window::Id,
//         services: &'a Services,
//     ) -> impl Into<Element<'a, Self::Message, Theme, iced_wgpu::Renderer>> {
//         Some(
//             self.dock_manager
//                 .view(window, services)?
//                 .map(MainViewMessage::Dock),
//         )
//     }

//     fn update(
//         &mut self,
//         message: Self::Message,
//         services: &mut Services,
//     ) -> impl Into<Task<Self::Message>> {
//         match message {
//             MainViewMessage::Dock(m) => self
//                 .dock_manager
//                 .update(m, services)
//                 .map(MainViewMessage::Dock),
//             MainViewMessage::WindowEvent(id, event) => {
//                 match event {
//                     window::Event::Focused => {
//                         self.actions_matcher.lock().reset_keyboard_state();
//                     }
//                     _ => {}
//                 }

//                 self.dock_manager.on_window_event(id, event).discard()
//             }

//             MainViewMessage::KeyboardEvent(window, event) => {
//                 if let Some(action) = self.actions_matcher.lock().on_keyboard_event(event) {
//                     if let Some(action_func) = services
//                         .service_mut::<ActionFunctionRegistry>()
//                         .get(action.clone())
//                     {
//                         log::info!("Triggering action: {}", action);
//                         return action_func.trigger(services).map(move |message| {
//                             MainViewMessage::ActionMessage(action.clone(), message)
//                         });
//                     } else {
//                         log::warn!("No action function found for action: {}", action);
//                     }
//                 }
//                 Task::none()
//             }
//             MainViewMessage::MouseEvent(window, event) => {
//                 match event {
//                     mouse::Event::CursorMoved { position } => {
//                         return self
//                             .dock_manager
//                             .on_cursor_moved(window, position)
//                             .map(MainViewMessage::Dock);
//                     }
//                     mouse::Event::ButtonReleased(mouse::Button::Left) => {
//                         return self
//                             .dock_manager
//                             .on_float_window_drag_end()
//                             .map(MainViewMessage::Dock);
//                     }
//                     _ => {}
//                 }

//                 Task::none()
//             }
//             MainViewMessage::CanvasCreated(e) => {
//                 log::info!("Canvas created: {}", e.id);
//                 let dock = CanvasDock::new(e.id, self.actions_matcher.clone());
//                 let id = <CanvasDock as Dock<Theme, Renderer>>::id(&dock);
//                 self.dock_manager.register_dock(dock);
//                 self.dock_manager.open_dock(id).map(MainViewMessage::Dock)
//             }
//             MainViewMessage::CanvasRemoved(e) => {
//                 log::info!("Canvas removed: {}", e.id);
//                 let id = DockId::new(construct_canvas_dock_id(e.id).into());
//                 self.dock_manager.unregister_dock(&id);

//                 Task::none()
//             }
//             MainViewMessage::ActionMessage(action_id, message) => {
//                 if let Some(action_func) = services
//                     .service_mut::<ActionFunctionRegistry>()
//                     .get(action_id.clone())
//                 {
//                     action_func
//                         .handle_message(message, services)
//                         .map(move |message| {
//                             MainViewMessage::ActionMessage(action_id.clone(), message)
//                         })
//                 } else {
//                     Task::none()
//                 }
//             }
//         }
//     }

//     fn close(self, services: &mut Services) -> Task<()> {
//         iced::exit()
//     }

//     fn subscription(&self) -> Subscription<Self::Message> {
//         let external = iced::event::listen_with(|event, status, window| match event {
//             iced::Event::Window(e) => Some(MainViewMessage::WindowEvent(window, e)),
//             iced::Event::Keyboard(e) => Some(MainViewMessage::KeyboardEvent(window, e)),
//             iced::Event::Mouse(e) => Some(MainViewMessage::MouseEvent(window, e)),
//             _ => None,
//         });

//         let dock = self.dock_manager.subscription().map(MainViewMessage::Dock);
//         let canvas_create = CanvasCreated::listen_to().map(MainViewMessage::CanvasCreated);
//         let canvas_remove = CanvasRemoved::listen_to().map(MainViewMessage::CanvasRemoved);

//         Subscription::batch([external, dock, canvas_create, canvas_remove])
//     }

//     fn windows(&self) -> Arc<[iced_core::window::Id]> {
//         self.dock_manager
//             .window_infos()
//             .map(|i| i.id)
//             .collect::<Vec<_>>()
//             .into()
//     }

//     fn root_window(&self) -> Option<iced_core::window::Id> {
//         Some(self.dock_manager.main_window().id)
//     }
// }
