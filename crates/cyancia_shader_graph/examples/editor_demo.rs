use cyancia_shader_graph::{
    editor::GraphEditor,
    graph::{Graph, GraphData, GraphResources},
    wgsl_std::{
        builtin_nodes, builtin_types,
        nodes::{GraphInputNode, GraphOutputNode},
    },
};
use gpui::{
    Action, App, AppContext, Context, Entity, IntoElement, Menu, MenuItem, ParentElement, Render,
    SharedString, Styled, Window, WindowOptions, div,
};
use gpui_component::{
    ActiveTheme, GlobalState, Root, Theme, ThemeMode, TitleBar, menu::AppMenuBar,
};
use gpui_platform::application;

#[derive(Default, Clone)]
struct DemoData {}

impl GraphData for DemoData {}

struct DemoEditor {
    menu_bar: Entity<AppMenuBar>,
    editor: Entity<GraphEditor<DemoData>>,
}

impl DemoEditor {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let mut nodes = builtin_nodes();
        nodes.register::<GraphInputNode>();
        nodes.register::<GraphOutputNode>();
        Self {
            menu_bar: menu_bar_init(cx),
            editor: cx.new(|cx| {
                GraphEditor::new(
                    Graph::new(GraphResources::default().into(), builtin_types().into()),
                    nodes.into(),
                    cx,
                )
            }),
        }
    }
}

impl Render for DemoEditor {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .w_full()
            .h_full()
            .child(TitleBar::new().child(self.menu_bar.clone()))
            .child(self.editor.clone())
    }
}

fn update_app_menu(app_menu_bar: Entity<AppMenuBar>, cx: &mut App) {
    cx.set_menus(build_menus(cx));
    let menus = build_menus(cx)
        .into_iter()
        .map(|menu| menu.owned())
        .collect();
    GlobalState::global_mut(cx).set_app_menus(menus);

    app_menu_bar.update(cx, |menu_bar, cx| {
        menu_bar.reload(cx);
    });
}

fn build_menus(cx: &App) -> Vec<Menu> {
    vec![Menu {
        name: "Themes".into(),
        items: vec![
            MenuItem::action("Light", SwitchThemeAction(ThemeMode::Light))
                .checked(!cx.theme().mode.is_dark()),
            MenuItem::action("Dark", SwitchThemeAction(ThemeMode::Dark))
                .checked(cx.theme().mode.is_dark()),
        ],
        disabled: false,
    }]
}

#[derive(Action, Clone, PartialEq)]
#[action(namespace = theme, no_json)]
struct SwitchThemeAction(ThemeMode);

fn menu_bar_init(cx: &mut App) -> Entity<AppMenuBar> {
    cx.on_action(|switch: &SwitchThemeAction, cx| {
        Theme::change(switch.0, None, cx);
        cx.refresh_windows();
    });

    let menu_bar = AppMenuBar::new(cx);
    update_app_menu(menu_bar.clone(), cx);
    cx.observe_global::<Theme>({
        let menu_bar = menu_bar.clone();
        move |cx| {
            update_app_menu(menu_bar.clone(), cx);
        }
    })
    .detach();

    menu_bar
}

fn main() {
    application()
        .with_assets(gpui_component_assets::Assets)
        .run(|cx| {
            gpui_component::init(cx);
            cyancia_widgets::init(cx);

            cx.open_window(
                WindowOptions {
                    titlebar: None,
                    ..Default::default()
                },
                |window, cx| {
                    let editor = cx.new(|cx| DemoEditor::new(window, cx));

                    cx.new(|cx| Root::new(editor, window, cx))
                },
            );
        });
}
