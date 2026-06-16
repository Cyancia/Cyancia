use std::collections::HashMap;

use cyancia_utils::wrapper;
use gpui::{App, BorrowAppContext, Global, Render, WindowHandle};
use gpui_component::Root;

pub fn init(cx: &mut App) {
    let vm = ViewManager::new(cx);
    cx.set_global(vm);
}

pub trait ViewAppExt {
    fn register_view<T: View>(&mut self);
    fn open_view(&mut self, id: ViewId) -> ViewOpenResult;
}

impl ViewAppExt for App {
    fn register_view<T: View>(&mut self) {
        self.global_mut::<ViewManager>().register_view::<T>();
    }

    fn open_view(&mut self, id: ViewId) -> ViewOpenResult {
        self.update_global::<ViewManager, _>(|manager, cx| manager.open_view(id, cx))
    }
}

pub trait View: Render + 'static {
    fn id() -> ViewId;
    fn open(cx: &mut App) -> anyhow::Result<WindowHandle<Root>>;
}

wrapper! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub ViewId : &'static str
}

pub struct ViewManager {
    main_view: Option<ViewId>,
    registered_views: HashMap<ViewId, Box<dyn Fn(&mut App) -> anyhow::Result<WindowHandle<Root>>>>,
    opened_view_root_window: HashMap<ViewId, WindowHandle<Root>>,
}

impl Global for ViewManager {}

#[derive(Debug)]
pub enum ViewOpenResult {
    AlreadyOpen(WindowHandle<Root>),
    New(WindowHandle<Root>),
    NotFound,
    Error(anyhow::Error),
}

impl ViewOpenResult {
    pub fn window_handle(self) -> Option<WindowHandle<Root>> {
        match self {
            Self::AlreadyOpen(window) | Self::New(window) => Some(window),
            Self::NotFound | Self::Error(_) => None,
        }
    }
}

impl ViewManager {
    pub fn new(cx: &mut App) -> Self {
        cx.on_window_closed(|cx, window| {
            let vm = cx.global_mut::<Self>();
            vm.opened_view_root_window
                .retain(|_, w| w.window_id() != window);

            if vm
                .main_view
                .as_ref()
                .is_some_and(|m| !vm.opened_view_root_window.contains_key(m))
            {
                cx.quit();
            }
        })
        .detach();

        Self {
            main_view: None,
            registered_views: HashMap::new(),
            opened_view_root_window: HashMap::new(),
        }
    }

    pub fn set_main_view(&mut self, id: ViewId) {
        self.main_view = Some(id);
    }

    pub fn register_view<T: View>(&mut self) {
        self.registered_views.insert(T::id(), Box::new(T::open));
    }

    pub fn open_view(&mut self, id: ViewId, cx: &mut App) -> ViewOpenResult {
        if let Some(window) = self.opened_view_root_window.get(&id) {
            return ViewOpenResult::AlreadyOpen(*window);
        }

        let Some(launch) = self.registered_views.get(&id) else {
            return ViewOpenResult::NotFound;
        };
        let window = match launch(cx) {
            Ok(entity) => entity,
            Err(err) => {
                return ViewOpenResult::Error(err);
            }
        };
        self.opened_view_root_window.insert(id, window);
        ViewOpenResult::New(window)
    }

    pub fn close_view(&mut self, id: ViewId, cx: &mut App) {
        if Some(id) == self.main_view {
            cx.quit();
            return;
        }

        if let Some(window) = self.opened_view_root_window.remove(&id) {
            let _ = window.update(cx, |_, window, _| {
                window.remove_window();
            });
        }
    }

    pub fn view_window_handle(&self, id: ViewId) -> Option<&WindowHandle<Root>> {
        self.opened_view_root_window.get(&id)
    }

    pub fn is_view_open(&self, id: ViewId) -> bool {
        self.opened_view_root_window.contains_key(&id)
    }
}
