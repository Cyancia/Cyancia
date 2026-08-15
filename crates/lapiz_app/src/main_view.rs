use std::{any::Any, sync::Arc};

use iced::keyboard::key;
use iced::{
    Element, Subscription, Task, Theme,
    keyboard::{self},
    mouse, window,
};
use iced_wgpu::Renderer;
use lapiz_actions::{
    ActionFunctionRegistry, ActionId,
    manifest::{ActionCollection, KeyBindingDefManifest},
};
use lapiz_assets::AssetAppExt;
use lapiz_canvas::{
    CanvasAppExt, CanvasManager,
    event::{CanvasCreated, CanvasRemoved},
    tools::PanTool,
};
use lapiz_dock::{
    DockManager, DockMessage,
    dock::{Dock, DockId},
};
use lapiz_input::key::KeyboardState;
use lapiz_runtime::{
    Services,
    event::Event,
    windows::{WindowView, WindowViewId},
};
use lapiz_tools::{ErasedToolFunctionMessage, GlobalToolBindings, ToolFunction, ToolProxies};

use crate::dock::{
    BRUSH_PRESETS_DOCK_ID, BrushPresetDock, COLOR_SELECTOR_DOCK_ID, CanvasDock, ColorSelectorDock,
    LAYER_DOCK_ID, LayersDock, TOOL_OPTIONS_DOCK_ID, ToolOptionsDock, construct_canvas_dock_id,
};

pub struct MainView {
    dock_manager: DockManager<Theme, Renderer>,
    action_collection: ActionCollection,
}

pub enum MainViewMessage {
    Dock(DockMessage),
    WindowEvent(window::Id, window::Event),
    KeyboardEvent(window::Id, keyboard::Event),
    MouseEvent(window::Id, mouse::Event),
    CanvasCreated(CanvasCreated),
    CanvasRemoved(CanvasRemoved),
    ActionMessage(ActionId, Box<dyn Any + Send + Sync>),
    ToolFunctionMessage(ErasedToolFunctionMessage),
}

impl MainView {
    fn switch_tool_keys(
        &mut self,
        services: &mut Services,
        is_keydown: bool,
    ) -> Task<MainViewMessage> {
        let tool_proxy = services
            .current_canvas()
            .map(|canvas| canvas.tool_proxy_id());

        if let Some(tool_proxy) = tool_proxy {
            services
                .service_scope::<ToolProxies, _>(|tool_proxies, services| {
                    let tool_proxy = tool_proxies.get_mut(&tool_proxy);
                    let keyboard_state = services.service::<KeyboardState>();
                    let seq = keyboard_state.get_sequence();

                    let config = services
                        .service::<GlobalToolBindings>()
                        .binding_for(seq)
                        .cloned();
                    let Some(config) = config else {
                        return tool_proxy.switch_override_tool(None, services);
                    };

                    if config.is_temporary {
                        tool_proxy.switch_override_tool(Some(config.tool.clone()), services)
                    } else if is_keydown {
                        tool_proxy.switch_tool(config.tool.clone(), services)
                    } else {
                        Task::none()
                    }
                })
                .map(MainViewMessage::ToolFunctionMessage)
        } else {
            Task::none()
        }
    }
}

impl WindowView for MainView {
    type Message = MainViewMessage;

    fn id() -> WindowViewId {
        WindowViewId::new("main_view")
    }

    fn boot(services: &mut Services) -> (Self, Task<Self::Message>) {
        let assets = services.assets();
        let manifests = assets.all_handles_of::<KeyBindingDefManifest>().unwrap();
        let manifest = manifests.first().unwrap().get().unwrap();

        log::info!(
            "Loading {} key bindings from manifest {}",
            manifest.actions.len(),
            manifest.name
        );
        let action_collection = ActionCollection::new(&manifest);

        let (main_window, task) = window::open(Default::default());
        let (mut dock_manager, dock_manager_task) = DockManager::new(main_window);
        dock_manager.register_dock(LayersDock::new());
        dock_manager.register_dock(ToolOptionsDock::new(services));
        dock_manager.register_dock(BrushPresetDock::new(services));
        dock_manager.register_dock(ColorSelectorDock::new(services));

        let dock_tasks = Task::batch([
            dock_manager.open_dock(DockId::new(COLOR_SELECTOR_DOCK_ID.into())),
            dock_manager.open_dock(DockId::new(LAYER_DOCK_ID.into())),
            dock_manager.open_dock(DockId::new(TOOL_OPTIONS_DOCK_ID.into())),
            dock_manager.open_dock(DockId::new(BRUSH_PRESETS_DOCK_ID.into())),
        ])
        .map(MainViewMessage::Dock);

        (
            Self {
                dock_manager,
                action_collection,
            },
            Task::batch([
                task.discard(),
                dock_manager_task.map(MainViewMessage::Dock),
                dock_tasks,
            ]),
        )
    }

    fn view<'a>(
        &'a self,
        window: window::Id,
        services: &'a Services,
    ) -> impl Into<Element<'a, Self::Message, Theme, iced_wgpu::Renderer>> {
        Some(
            self.dock_manager
                .view(window, services)?
                .map(MainViewMessage::Dock),
        )
    }

    fn update(
        &mut self,
        message: Self::Message,
        services: &mut Services,
    ) -> impl Into<Task<Self::Message>> {
        match message {
            MainViewMessage::Dock(m) => self
                .dock_manager
                .update(m, services)
                .map(MainViewMessage::Dock),
            MainViewMessage::WindowEvent(id, event) => {
                self.dock_manager.on_window_event(id, event).discard()
            }

            MainViewMessage::KeyboardEvent(_window, event) => {
                let keyboard_state = services.service_mut::<KeyboardState>();
                let old_modifier_count = keyboard_state.modifiers().bits().count_ones();

                match &event {
                    keyboard::Event::KeyPressed {
                        physical_key: key::Physical::Code(code),
                        repeat: false,
                        ..
                    } => {
                        if *code == key::Code::ControlLeft
                            || *code == key::Code::ControlRight
                            || *code == key::Code::ShiftLeft
                            || *code == key::Code::ShiftRight
                            || *code == key::Code::AltLeft
                            || *code == key::Code::AltRight
                            || *code == key::Code::SuperLeft
                            || *code == key::Code::SuperRight
                            || *code == key::Code::Meta
                        {
                            return Task::none();
                        }
                        keyboard_state.press(*code);

                        if let Some(action) = self
                            .action_collection
                            .get_action_id(keyboard_state.get_sequence())
                            && let Some(action_func) = services
                                .service_mut::<ActionFunctionRegistry>()
                                .get(action.clone())
                        {
                            log::info!("Triggering action: {}", action.0);
                            return action_func.trigger(services).map(move |message| {
                                MainViewMessage::ActionMessage(action.clone(), message)
                            });
                        }

                        self.switch_tool_keys(services, true)
                    }
                    keyboard::Event::KeyReleased {
                        physical_key: key::Physical::Code(code),
                        ..
                    } => {
                        keyboard_state.release(*code);
                        self.switch_tool_keys(services, false)
                    }
                    keyboard::Event::ModifiersChanged(modifiers) => {
                        keyboard_state.set_modifiers(*modifiers);

                        let new_modifier_count = keyboard_state.modifiers().bits().count_ones();
                        let is_keydown = new_modifier_count > old_modifier_count;
                        self.switch_tool_keys(services, is_keydown)
                    }
                    _ => Task::none(),
                }
            }
            MainViewMessage::MouseEvent(window, event) => {
                match event {
                    mouse::Event::CursorMoved { position } => {
                        return self
                            .dock_manager
                            .on_cursor_moved(window, position)
                            .map(MainViewMessage::Dock);
                    }
                    mouse::Event::ButtonReleased(mouse::Button::Left) => {
                        return self
                            .dock_manager
                            .on_float_window_drag_end()
                            .map(MainViewMessage::Dock);
                    }
                    _ => {}
                }

                Task::none()
            }
            MainViewMessage::CanvasCreated(e) => {
                log::info!("Canvas created: {}", e.id);
                let tool_proxy_id = services
                    .service::<CanvasManager>()
                    .get(&e.id)
                    .unwrap()
                    .tool_proxy_id();
                let tool_task =
                    services.service_scope::<ToolProxies, _>(|tool_proxies, services| {
                        tool_proxies
                            .get_mut(&tool_proxy_id)
                            .switch_tool(PanTool::id(), services)
                    });
                let dock = CanvasDock::new(e.id, tool_proxy_id, self.dock_manager.main_window().id);
                let id = <CanvasDock as Dock<Theme, Renderer>>::id(&dock);
                self.dock_manager.register_dock(dock);
                Task::batch([
                    tool_task.map(MainViewMessage::ToolFunctionMessage),
                    self.dock_manager.open_dock(id).map(MainViewMessage::Dock),
                ])
            }
            MainViewMessage::CanvasRemoved(e) => {
                log::info!("Canvas removed: {}", e.id);
                let id = DockId::new(construct_canvas_dock_id(e.id).into());
                self.dock_manager.unregister_dock(&id);

                Task::none()
            }
            MainViewMessage::ActionMessage(action_id, message) => {
                if let Some(action_func) = services
                    .service_mut::<ActionFunctionRegistry>()
                    .get(action_id.clone())
                {
                    action_func
                        .handle_message(message, services)
                        .map(move |message| {
                            MainViewMessage::ActionMessage(action_id.clone(), message)
                        })
                } else {
                    Task::none()
                }
            }
            MainViewMessage::ToolFunctionMessage(message) => {
                let Some(canvas) = services.current_canvas() else {
                    return Task::none();
                };

                let tool_proxy_id = canvas.tool_proxy_id();
                services
                    .service_scope::<ToolProxies, _>(|tool_proxies, services| {
                        tool_proxies
                            .get_mut(&tool_proxy_id)
                            .handle_message(message, services)
                    })
                    .map(MainViewMessage::ToolFunctionMessage)
            }
        }
    }

    fn close(self, _services: &mut Services) -> Task<()> {
        iced::exit()
    }

    fn subscription(&self, services: &Services) -> Subscription<Self::Message> {
        let external = iced::event::listen_with(|event, _status, window| match event {
            iced::Event::Window(e) => Some(MainViewMessage::WindowEvent(window, e)),
            iced::Event::Keyboard(e) => Some(MainViewMessage::KeyboardEvent(window, e)),
            iced::Event::Mouse(e) => Some(MainViewMessage::MouseEvent(window, e)),
            _ => None,
        });

        let dock = self
            .dock_manager
            .subscription(services)
            .map(MainViewMessage::Dock);
        let canvas_create = CanvasCreated::listen_to().map(MainViewMessage::CanvasCreated);
        let canvas_remove = CanvasRemoved::listen_to().map(MainViewMessage::CanvasRemoved);

        Subscription::batch([external, dock, canvas_create, canvas_remove])
    }

    fn windows(&self) -> Arc<[iced_core::window::Id]> {
        self.dock_manager
            .window_infos()
            .map(|i| i.id)
            .chain(self.dock_manager.sub_windows())
            .collect::<Vec<_>>()
            .into()
    }

    fn root_window(&self) -> Option<iced_core::window::Id> {
        Some(self.dock_manager.main_window().id)
    }
}
